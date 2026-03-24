#!/usr/bin/env python3

from __future__ import annotations

import argparse
import difflib
import os
import select
import shutil
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Callable

EXPECT_TIMEOUT_HEADER = b"# expect: timeout\n"
ANSI_RED = "\033[31m"
ANSI_GREEN = "\033[32m"
ANSI_RESET = "\033[0m"

# Examples that are expected to compile successfully and have .expect output files
EXPECTED_TO_COMPILE: list[str] = [
    "examples/test_break_inner_loop.lang",
    "examples/test_defer.lang",
    "examples/test_defer_bang.lang",
    "examples/test_duplicate_import.lang",
    "examples/test_features.lang",
    "examples/test_ffi2.lang",
    "examples/test_fndec_as_type.lang",
    "examples/test_for_stmt.lang",
    "examples/test_hello_world.lang",
    "examples/test_if_else_stmt.lang",
    "examples/test_import_error.lang",
    "examples/test_math_cal.lang",
    "examples/test_my_allocator.lang",
    "examples/test_one_for.lang",
    "examples/test_operators.lang",
    "examples/test_rawptr_basic.lang",
    "examples/test_rawptr_call.lang",
    "examples/test_rawptr_declare.lang",
    "examples/test_rawptr_minimal.lang",
    "examples/test_rawptr_simple.lang",
    "examples/test_return_simple.lang",
    "examples/test_return_simple2.lang",
    "examples/test_simple_ret.lang",
    "examples/test_try_consume.lang",
    "examples/test_try_discard.lang",
    "examples/test_try_void.lang",
    "examples/test_tuple.lang",
    "examples/test_tuple_ret.lang",
    "examples/test_var_no_type.lang",
    "examples/test_var_reassign.lang",
    "examples/test_optional.lang",
    "examples/test_switch_stmt.lang",
    "examples/test_error.lang",
    "examples/test_import_stmt.lang",
]

# Examples that are intentionally designed to fail compilation (error test cases)
EXPECTED_TO_FAIL_COMPILE: list[str] = [
    "examples/test_try_consume_error.lang",  # Invalid: try without consuming return value
    "examples/test_try_tuple.lang",  # Invalid: try on tuple return type
    "examples/test_switch_non_exhaust.lang",  # Non-exhaustive switch should error
    "examples/test_builtin_conflict.lang",  # Cannot override builtin functions
    "examples/test_rawptr_isnull_only.lang",  # is_null requires rawptr, not i64
    "examples/test_const_reassign_error.lang",  # const reassignment should error
    "examples/test_var_no_init2.lang",  # var without init should error
    "examples/test_var_no_init3.lang",  # var with empty init should error
    "examples/test_array_decl.lang",  # element value out of range of type
    "examples/test_catch_check.lang", # try/catch cannot catch non-error types
    "examples/test_unmatch_if_expr.lang", # if/else blocks with mismatched types
    "examples/test_without_import.lang", # using std library without import should fail
]


@dataclass
class ExampleResult:
    source: Path
    expect: Path
    compile_ok: bool
    compile_output: str
    runtime_ok: bool | None = None
    runtime_state: str | None = None
    runtime_output: bytes | None = None
    runtime_returncode: int | None = None
    runtime_error: str | None = None
    runtime_output_truncated: bool = False
    expect_exists: bool = False
    expect_matches: bool | None = None


@dataclass
class ProgressReporter:
    enabled: bool
    rendered_width: int = 0

    def update(self, message: str) -> None:
        if not self.enabled:
            return
        padding = ""
        if self.rendered_width > len(message):
            padding = " " * (self.rendered_width - len(message))
        sys.stderr.write(f"\r{message}{padding}")
        sys.stderr.flush()
        self.rendered_width = len(message)

    def clear(self) -> None:
        if not self.enabled or self.rendered_width == 0:
            return
        sys.stderr.write("\r" + (" " * self.rendered_width) + "\r")
        sys.stderr.flush()
        self.rendered_width = 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Classify examples by compile status and manage .expect files for "
            "successfully compiled examples."
        )
    )
    parser.add_argument(
        "command",
        choices=("list", "check", "update"),
        nargs="?",
        default="list",
        help="list statuses, check .expect outputs, or update .expect outputs",
    )
    parser.add_argument(
        "paths",
        nargs="*",
        help=(
            "Optional .lang files or directories to inspect. "
            "Defaults to example entrypoints under examples/."
        ),
    )
    parser.add_argument(
        "--compiler",
        type=Path,
        help="Use an existing lang compiler binary instead of building target/debug/lang.",
    )
    parser.add_argument(
        "--all-lang",
        action="store_true",
        help="Treat every .lang file under examples/ as an entrypoint.",
    )
    parser.add_argument(
        "--timeout",
        type=float,
        default=5.0,
        help="Seconds to allow each compiled example to run before failing.",
    )
    parser.add_argument(
        "--max-output-bytes",
        type=int,
        default=65536,
        help="Maximum runtime output bytes to capture per example.",
    )
    parser.add_argument(
        "--show-errors",
        action="store_true",
        help="Print compile/runtime error details.",
    )
    parser.add_argument(
        "--progress",
        dest="progress",
        action="store_true",
        help="Print compiler/example progress updates to stderr.",
    )
    parser.add_argument(
        "--no-progress",
        dest="progress",
        action="store_false",
        help="Disable compiler/example progress updates.",
    )
    parser.add_argument(
        "--progress-interval",
        type=float,
        default=1.0,
        help="Seconds between runtime heartbeat updates for the current example.",
    )
    parser.set_defaults(progress=sys.stderr.isatty())
    if hasattr(parser, "parse_intermixed_args"):
        return parser.parse_intermixed_args()
    return parser.parse_args()


def repo_root() -> Path:
    return Path(__file__).resolve().parent.parent


def default_entrypoints(examples_dir: Path, include_all_lang: bool) -> list[Path]:
    if include_all_lang:
        return sorted(path for path in examples_dir.rglob("*.lang"))

    discovered: list[Path] = sorted(path for path in examples_dir.glob("*.lang"))
    discovered.extend(
        sorted(
            path
            for path in examples_dir.rglob("main.lang")
            if path.parent != examples_dir
        )
    )

    unique: list[Path] = []
    seen: set[Path] = set()
    for path in discovered:
        if path in seen:
            continue
        seen.add(path)
        unique.append(path)
    return unique


def resolve_requested_paths(
    requested: list[str], examples_dir: Path, include_all_lang: bool
) -> list[Path]:
    if not requested:
        return default_entrypoints(examples_dir, include_all_lang)

    resolved: list[Path] = []
    seen: set[Path] = set()
    for raw in requested:
        path = Path(raw)
        if not path.is_absolute():
            path = (repo_root() / path).resolve()
        else:
            path = path.resolve()

        if not path.exists():
            raise SystemExit(f"Path does not exist: {raw}")

        if path.is_dir():
            candidates = default_entrypoints(path, include_all_lang)
        else:
            if path.suffix != ".lang":
                raise SystemExit(f"Expected a .lang file or directory: {raw}")
            candidates = [path]

        for candidate in candidates:
            try:
                candidate.relative_to(examples_dir)
            except ValueError as exc:
                raise SystemExit(
                    f"Only files under {examples_dir} are supported: {candidate}"
                ) from exc
            if candidate in seen:
                continue
            seen.add(candidate)
            resolved.append(candidate)

    return sorted(resolved)


def ensure_compiler(
    root: Path, compiler: Path | None, progress: ProgressReporter
) -> Path:
    if compiler is not None:
        compiler_path = compiler if compiler.is_absolute() else (root / compiler)
        compiler_path = compiler_path.resolve()
        if not compiler_path.exists():
            raise SystemExit(f"Compiler does not exist: {compiler_path}")
        return compiler_path

    build = subprocess.run(
        ["cargo", "build", "--quiet"],
        cwd=root,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )
    if build.returncode != 0:
        progress.clear()
        sys.stdout.write(build.stdout)
        raise SystemExit(build.returncode)

    compiler_path = root / "target" / "debug" / "lang"
    if not compiler_path.exists():
        raise SystemExit(f"Expected compiler at: {compiler_path}")
    return compiler_path


def sanitize_binary_name(root: Path, source: Path) -> str:
    rel = source.relative_to(root / "examples")
    parts = list(rel.with_suffix("").parts)
    return "__".join(parts)


def compare_bytes(expected: bytes, actual: bytes) -> str:
    expected_text = expected.decode("utf-8", errors="replace").splitlines(keepends=True)
    actual_text = actual.decode("utf-8", errors="replace").splitlines(keepends=True)
    diff = difflib.unified_diff(
        expected_text,
        actual_text,
        fromfile="expected",
        tofile="actual",
        n=3,
    )
    return "".join(diff)


def parse_expect_bytes(raw: bytes) -> tuple[str, bytes]:
    if raw.startswith(EXPECT_TIMEOUT_HEADER):
        return ("timeout", raw[len(EXPECT_TIMEOUT_HEADER) :])
    return ("completed", raw)


def supports_color() -> bool:
    if os.environ.get("CLICOLOR_FORCE") == "1":
        return True
    if os.environ.get("NO_COLOR") is not None:
        return False
    return sys.stdout.isatty() and os.environ.get("TERM") != "dumb"


def colorize(text: str, color: str) -> str:
    if not supports_color():
        return text
    return f"{color}{text}{ANSI_RESET}"


def display_path(path: Path, root: Path) -> Path:
    try:
        return path.relative_to(root)
    except ValueError:
        return path


def format_duration(seconds: float) -> str:
    if seconds < 1:
        return f"{seconds * 1000:.0f}ms"
    if seconds < 10:
        return f"{seconds:.2f}s"
    if seconds < 60:
        return f"{seconds:.1f}s"
    minutes, remainder = divmod(seconds, 60)
    if minutes < 60:
        return f"{int(minutes)}m{remainder:04.1f}s"
    hours, minutes = divmod(minutes, 60)
    return f"{int(hours)}h{int(minutes):02d}m{remainder:02.0f}s"


def format_case_status(ok: bool) -> str:
    if ok:
        return colorize("ok", ANSI_GREEN)
    return colorize("failed", ANSI_RED)


def evaluate_expect_match(result: ExampleResult) -> None:
    if not result.expect_exists or result.runtime_output is None:
        return

    expected_state, expected_output = parse_expect_bytes(result.expect.read_bytes())
    result.expect_matches = (
        result.runtime_state == expected_state and result.runtime_output == expected_output
    )


def run_examples(
    root: Path,
    compiler: Path,
    sources: list[Path],
    timeout_seconds: float,
    max_output_bytes: int,
    progress: ProgressReporter,
    progress_interval: float,
) -> list[ExampleResult]:
    results: list[ExampleResult] = []
    examples_dir = root / "examples"
    total = len(sources)

    with tempfile.TemporaryDirectory(prefix="lang-example-check-") as temp_dir_raw:
        temp_dir = Path(temp_dir_raw)
        temp_examples = temp_dir / "examples"
        temp_bins = temp_dir / "bin"
        shutil.copytree(examples_dir, temp_examples)
        temp_bins.mkdir(parents=True, exist_ok=True)

        for index, source in enumerate(sources, start=1):
            rel = source.relative_to(examples_dir)
            temp_source = temp_examples / rel
            expect_path = source.with_suffix(".expect")
            binary_path = temp_bins / sanitize_binary_name(root, source)
            rel_source = display_path(source, root)

            def report_runtime_progress(
                elapsed: float,
                captured_bytes: int,
                truncated: bool,
                *,
                rel_source: Path = rel_source,
                index: int = index,
            ) -> None:
                progress.update(
                    (
                        f"[{index}/{total}] still running {rel_source} "
                        f"for {format_duration(elapsed)} "
                        f"(captured {captured_bytes} bytes"
                        f"{', truncated' if truncated else ''})"
                    ),
                )

            progress.update(f"[{index}/{total}] compiling {rel_source}")
            compile_started_at = time.monotonic()
            compile_proc = subprocess.run(
                [str(compiler), "build", str(temp_source), "-o", str(binary_path)],
                cwd=root,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
            )
            compile_elapsed = time.monotonic() - compile_started_at

            result = ExampleResult(
                source=source,
                expect=expect_path,
                compile_ok=compile_proc.returncode == 0,
                compile_output=compile_proc.stdout,
                expect_exists=expect_path.exists(),
            )

            if not result.compile_ok:
                progress.update(
                    (
                        f"[{index}/{total}] compile failed {rel_source} "
                        f"after {format_duration(compile_elapsed)}"
                    ),
                )
                results.append(result)
                continue

            try:
                progress.update(
                    (
                        f"[{index}/{total}] running {rel_source} "
                        f"(compile {format_duration(compile_elapsed)})"
                    ),
                )
                runtime_started_at = time.monotonic()
                runtime_state, runtime_output, runtime_returncode, output_truncated = run_binary(
                    [str(binary_path)],
                    root,
                    timeout_seconds,
                    max_output_bytes,
                    on_progress=report_runtime_progress,
                    progress_interval=progress_interval,
                )
                runtime_elapsed = time.monotonic() - runtime_started_at
                result.runtime_state = runtime_state
                result.runtime_ok = runtime_state == "completed"
                result.runtime_output = runtime_output
                result.runtime_returncode = runtime_returncode
                result.runtime_output_truncated = output_truncated
                if runtime_state == "signaled" and runtime_returncode is not None:
                    result.runtime_error = f"terminated by signal {-runtime_returncode}"
                elif runtime_state == "timeout":
                    result.runtime_error = f"timed out after {timeout_seconds:g}s"
                evaluate_expect_match(result)
            except OSError as exc:
                runtime_elapsed = time.monotonic() - runtime_started_at
                result.runtime_ok = False
                result.runtime_state = "signaled"
                result.runtime_output = b""
                result.runtime_error = str(exc)
                evaluate_expect_match(result)

            ok, detail = compiled_case_summary(result)
            suffix = f" ({detail})" if detail else ""
            status_text = "ok" if ok else "failed"
            progress.update(
                (
                    f"[{index}/{total}] finished {rel_source} "
                    f"[{status_text}] in {format_duration(runtime_elapsed)}{suffix}"
                ),
            )
            results.append(result)

    return results


def run_binary(
    argv: list[str],
    cwd: Path,
    timeout_seconds: float,
    max_output_bytes: int,
    on_progress: Callable[[float, int, bool], None] | None = None,
    progress_interval: float = 1.0,
) -> tuple[str, bytes, int | None, bool]:
    proc = subprocess.Popen(
        argv,
        cwd=cwd,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    output = bytearray()
    output_truncated = False
    started_at = time.monotonic()
    deadline = started_at + timeout_seconds
    next_progress_at = (
        started_at + progress_interval
        if on_progress is not None and progress_interval > 0
        else None
    )

    try:
        while True:
            if proc.stdout is None:
                break

            now = time.monotonic()
            if next_progress_at is not None and now >= next_progress_at:
                on_progress(now - started_at, len(output), output_truncated)
                next_progress_at = now + progress_interval

            remaining = deadline - now
            if remaining <= 0:
                proc.kill()
                proc.wait()
                return ("timeout", bytes(output), None, output_truncated)

            ready, _, _ = select.select([proc.stdout], [], [], min(0.05, remaining))
            if ready:
                chunk = os.read(proc.stdout.fileno(), 4096)
                if chunk:
                    if len(output) < max_output_bytes:
                        take = min(len(chunk), max_output_bytes - len(output))
                        output.extend(chunk[:take])
                        if take < len(chunk):
                            output_truncated = True
                    else:
                        output_truncated = True
                elif proc.poll() is not None:
                    break
            elif proc.poll() is not None:
                break

        if proc.stdout is not None:
            while True:
                chunk = os.read(proc.stdout.fileno(), 4096)
                if not chunk:
                    break
                if len(output) < max_output_bytes:
                    take = min(len(chunk), max_output_bytes - len(output))
                    output.extend(chunk[:take])
                    if take < len(chunk):
                        output_truncated = True
                else:
                    output_truncated = True

        returncode = proc.wait()
        state = "completed" if returncode >= 0 else "signaled"
        return (state, bytes(output), returncode, output_truncated)
    finally:
        if proc.poll() is None:
            proc.kill()
            proc.wait()


def write_expect_files(results: list[ExampleResult]) -> int:
    updated = 0
    for result in results:
        if not result.compile_ok or result.runtime_output is None:
            continue
        if result.runtime_state == "completed" and result.runtime_output_truncated:
            continue
        if result.runtime_state == "completed":
            contents = result.runtime_output
        elif result.runtime_state == "timeout":
            contents = EXPECT_TIMEOUT_HEADER + result.runtime_output
        else:
            continue
        result.expect.write_bytes(contents)
        result.expect_exists = True
        result.expect_matches = True
        updated += 1
    return updated


def compiled_case_summary(result: ExampleResult) -> tuple[bool, str | None]:
    if result.runtime_state == "signaled":
        return (False, "runtime failed")
    if result.runtime_state == "completed" and result.runtime_output_truncated:
        return (False, "runtime output truncated")
    if result.runtime_state == "timeout" and not result.expect_exists:
        return (False, "missing .expect (timeout)")
    if not result.expect_exists:
        return (False, "missing .expect")
    if result.expect_matches is False:
        return (False, "expect mismatch")
    return (True, None)


def print_results(results: list[ExampleResult], root: Path, show_errors: bool) -> None:
    compiled = [result for result in results if result.compile_ok]
    failed = [result for result in results if not result.compile_ok]

    print(f"Compiled successfully ({len(compiled)}):")
    for result in compiled:
        rel_source = result.source.relative_to(root)
        rel_expect = result.expect.relative_to(root)
        ok, detail = compiled_case_summary(result)
        status = format_case_status(ok)
        suffix = f" {detail}" if detail else ""
        print(f"- {rel_source} -> {rel_expect} [{status}]{suffix}")
        if show_errors and result.runtime_ok is False:
            detail = result.runtime_error or f"exit code {result.runtime_returncode}"
            print(f"  {detail}")

    print()
    print(f"Compilation failed ({len(failed)}):")
    for result in failed:
        rel_source = result.source.relative_to(root)
        print(f"- {rel_source} [{format_case_status(False)}]")
        if show_errors:
            snippet = result.compile_output.strip()
            if snippet:
                for line in snippet.splitlines():
                    print(f"  {line}")


def print_check_failures(results: list[ExampleResult], root: Path) -> None:
    for result in results:
        rel_source = result.source.relative_to(root)
        if result.compile_ok and result.runtime_ok and result.expect_exists and result.expect_matches is False:
            print()
            print(f"Mismatch: {rel_source}")
            expected = result.expect.read_bytes()
            actual = result.runtime_output or b""
            diff = compare_bytes(expected, actual)
            if diff:
                sys.stdout.write(diff)


def validate_expectations(results: list[ExampleResult], root: Path) -> tuple[list[ExampleResult], list[ExampleResult], list[ExampleResult], list[ExampleResult], list[ExampleResult]]:
    """
    Validate compilation results against expected lists.
    Returns:
        - expected_compile_ok: Files expected to compile and did compile
        - expected_compile_fail: Files expected to fail and did fail
        - unexpected_compile_ok: Files NOT expected to compile but did compile (should not happen)
        - unexpected_compile_fail: Files NOT expected to compile but failed (need to fix)
        - unclassified: Files not in either list
    """
    expected_compile_ok: list[ExampleResult] = []
    expected_compile_fail: list[ExampleResult] = []
    unexpected_compile_ok: list[ExampleResult] = []  # Expected fail but compiled (shouldn't happen)
    unexpected_compile_fail: list[ExampleResult] = []  # Expected to compile but failed
    unclassified: list[ExampleResult] = []  # Not in either list

    for result in results:
        rel_path = str(result.source.relative_to(root))
        
        if rel_path in EXPECTED_TO_COMPILE:
            if result.compile_ok:
                expected_compile_ok.append(result)
            else:
                unexpected_compile_fail.append(result)
        elif rel_path in EXPECTED_TO_FAIL_COMPILE:
            if not result.compile_ok:
                expected_compile_fail.append(result)
            else:
                unexpected_compile_ok.append(result)
        else:
            # Not in either list - treat as unclassified
            unclassified.append(result)

    return expected_compile_ok, expected_compile_fail, unexpected_compile_ok, unexpected_compile_fail, unclassified


def print_expectation_summary(
    expected_compile_ok: list[ExampleResult],
    expected_compile_fail: list[ExampleResult],
    unexpected_compile_ok: list[ExampleResult],
    unexpected_compile_fail: list[ExampleResult],
    unclassified: list[ExampleResult],
    root: Path,
) -> None:
    print()
    print("=" * 60)
    print("EXPECTATION VALIDATION SUMMARY")
    print("=" * 60)
    
    print()
    print(f"Expected to compile and compiled OK ({len(expected_compile_ok)}):")
    for result in expected_compile_ok:
        rel_source = result.source.relative_to(root)
        rel_expect = result.expect.relative_to(root)
        ok, detail = compiled_case_summary(result)
        status = format_case_status(ok)
        suffix = f" {detail}" if detail else ""
        print(f"  - {rel_source} -> {rel_expect} [{status}]{suffix}")
    
    print()
    print(f"Expected to fail and failed as expected ({len(expected_compile_fail)}):")
    for result in expected_compile_fail:
        rel_source = result.source.relative_to(root)
        print(f"  - {rel_source} [{format_case_status(True)}] (expected compile error)")
    
    if unexpected_compile_ok:
        print()
        print(f"UNEXPECTED: Compiled but expected to fail ({len(unexpected_compile_ok)}):")
        for result in unexpected_compile_ok:
            rel_source = result.source.relative_to(root)
            print(f"  - {rel_source} [{format_case_status(True)}]")
    
    if unexpected_compile_fail:
        print()
        print(f"UNEXPECTED: Failed to compile but expected to succeed ({len(unexpected_compile_fail)}):")
        for result in unexpected_compile_fail:
            rel_source = result.source.relative_to(root)
            print(f"  - {rel_source} [{format_case_status(False)}]")
    
    if unclassified:
        print()
        print(f"Unclassified (not in any list) ({len(unclassified)}):")
        for result in unclassified:
            rel_source = result.source.relative_to(root)
            status = format_case_status(result.compile_ok)
            print(f"  - {rel_source} [{status}]")
    
    print()
    print("=" * 60)


def has_check_failures(results: list[ExampleResult]) -> bool:
    for result in results:
        if not result.compile_ok and result.expect_exists:
            return True
        if result.compile_ok and result.runtime_state == "signaled":
            return True
        if result.compile_ok and result.runtime_state == "completed" and result.runtime_output_truncated:
            return True
        if result.compile_ok and not result.expect_exists:
            return True
        if result.compile_ok and result.expect_exists and result.expect_matches is False:
            return True
    return False


def main() -> int:
    args = parse_args()
    root = repo_root()
    examples_dir = root / "examples"
    sources = resolve_requested_paths(args.paths, examples_dir, args.all_lang)
    progress = ProgressReporter(args.progress)

    try:
        progress.update(f"Building compiler for {len(sources)} example(s)...")
        compiler = ensure_compiler(root, args.compiler, progress)
        progress.update(f"Using compiler: {display_path(compiler, root)}")
        results = run_examples(
            root,
            compiler,
            sources,
            args.timeout,
            args.max_output_bytes,
            progress,
            args.progress_interval,
        )
    finally:
        progress.clear()

    if args.command == "update":
        updated = write_expect_files(results)
        print(f"Updated {updated} .expect file(s).")
        print()

    # Validate expectations using the two lists
    expected_compile_ok, expected_compile_fail, unexpected_compile_ok, unexpected_compile_fail, unclassified = validate_expectations(results, root)
    
    # Only show full results for check/update commands, skip for list command
    if args.command != "list":
        print_results(results, root, args.show_errors)
    
    # Always show expectation validation summary
    print_expectation_summary(
        expected_compile_ok,
        expected_compile_fail,
        unexpected_compile_ok,
        unexpected_compile_fail,
        unclassified,
        root,
    )

    if args.command == "check":
        print_check_failures(results, root)
        return 1 if has_check_failures(results) else 0

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
