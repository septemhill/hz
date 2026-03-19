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

EXPECT_TIMEOUT_HEADER = b"# expect: timeout\n"
ANSI_RED = "\033[31m"
ANSI_GREEN = "\033[32m"
ANSI_RESET = "\033[0m"


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


def ensure_compiler(root: Path, compiler: Path | None) -> Path:
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
) -> list[ExampleResult]:
    results: list[ExampleResult] = []
    examples_dir = root / "examples"

    with tempfile.TemporaryDirectory(prefix="lang-example-check-") as temp_dir_raw:
        temp_dir = Path(temp_dir_raw)
        temp_examples = temp_dir / "examples"
        temp_bins = temp_dir / "bin"
        shutil.copytree(examples_dir, temp_examples)
        temp_bins.mkdir(parents=True, exist_ok=True)

        for source in sources:
            rel = source.relative_to(examples_dir)
            temp_source = temp_examples / rel
            expect_path = source.with_suffix(".expect")
            binary_path = temp_bins / sanitize_binary_name(root, source)

            compile_proc = subprocess.run(
                [str(compiler), "build", str(temp_source), "-o", str(binary_path)],
                cwd=root,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
            )

            result = ExampleResult(
                source=source,
                expect=expect_path,
                compile_ok=compile_proc.returncode == 0,
                compile_output=compile_proc.stdout,
                expect_exists=expect_path.exists(),
            )

            if not result.compile_ok:
                results.append(result)
                continue

            try:
                runtime_state, runtime_output, runtime_returncode, output_truncated = run_binary(
                    [str(binary_path)],
                    root,
                    timeout_seconds,
                    max_output_bytes,
                )
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
                result.runtime_ok = False
                result.runtime_state = "signaled"
                result.runtime_output = b""
                result.runtime_error = str(exc)
                evaluate_expect_match(result)

            results.append(result)

    return results


def run_binary(
    argv: list[str], cwd: Path, timeout_seconds: float, max_output_bytes: int
) -> tuple[str, bytes, int | None, bool]:
    proc = subprocess.Popen(
        argv,
        cwd=cwd,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    output = bytearray()
    output_truncated = False
    deadline = time.monotonic() + timeout_seconds

    try:
        while True:
            if proc.stdout is None:
                break

            remaining = deadline - time.monotonic()
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
    compiler = ensure_compiler(root, args.compiler)
    results = run_examples(
        root,
        compiler,
        sources,
        args.timeout,
        args.max_output_bytes,
    )

    if args.command == "update":
        updated = write_expect_files(results)
        print(f"Updated {updated} .expect file(s).")
        print()

    print_results(results, root, args.show_errors)

    if args.command == "check":
        print_check_failures(results, root)
        return 1 if has_check_failures(results) else 0

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
