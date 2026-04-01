# Lang Programming Language

<p align="center">
  <strong>A system programming language targeting macOS with LLVM backend</strong>
</p>

<p align="center">
  <a href="https://github.com/septemlee/lang">
    <img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License">
  </a>
  <img src="https://img.shields.io/badge/Rust-2024-edc3a0?style=flat&logo=rust" alt="Rust Edition">
  <img src="https://img.shields.io/badge/LLVM-21-blue.svg" alt="LLVM 21">
</p>

## Overview

TBD

## Features

- **Strong Type System**: Integer types (i8-i64, u8-u64), floats (f32, f64), booleans, void, strings, and raw pointers
- **Raw Pointers**: `rawptr` type for FFI and low-level memory operations (similar to C's void*)
- **Optional Types**: Nullable types using `?` prefix (e.g., `?i32`, `?bool`) with `if/else` pattern matching
- **Error Handling**: Result types with `!` suffix (e.g., `i64!`), `try` and `catch` operators
- **Error Types**: Algebraic error types with `error` keyword and union types using `|`
- **Structs**: User-defined data types with methods and `pub` visibility
- **Interfaces**: Polymorphism through trait-like interfaces
- **Enums**: Algebraic data types with generic parameters (e.g., `enum<T>`)
- **Tuples**: Multiple values in a single type (e.g., `(i64, i64, i64)`) with destructuring
- **Arrays**: Fixed-size arrays (e.g., `[3]u8`, `[3]u8{1, 2, 3}`)
- **Functions**: First-class functions with typed parameters and return values
- **Generics**: Generic type parameters with compile-time type instantiation (e.g., `Compose<T>`, `add<T>`)
- **Control Flow**: `if/else`, `for` (range, array, iterator, infinite, conditional), `switch`
- **Method Syntax**: Object-oriented programming with `self` parameter
- **Modules**: Import system for code organization
- **Visibility**: `pub` keyword for public functions, fields, and types
- **Defer**: Deferred function calls with `defer` and `defer!` (with error propagation)
- **FFI**: External C function calls with `external cdecl`
- **Built-in Functions**: `@is_null()` and `@is_not_null()` for rawptr checks
- **Operators**: Arithmetic (+, -, *, /, %), bitwise (<<, >>, &, |, ^), comparison, logical (&&, ||, !)
- **Testing**: Built-in testing assertions

## Language Syntax

### Hello World

```lang
import "io"

fn main() void {
    io.println("Hello, World!");
}
```

### Variables

```lang
// Immutable variable
const name: String = "Lang";

// Mutable variable
var count: i32 = 0;
count += 1;
```

### Functions

```lang
fn add(a: i32, b: i32) i32 {
    return a + b;
}

// Function with error return type
fn maybeFail() i64! {
    return 1;
}

fn main() i64 {
    const result = add(1, 2);
    return result;
}
```

### Structs

```lang
struct Person {
    name: String,
    age: i32,

    pub fn new(name: String, age: i32) Person {
        return Person{ name, age };
    }

    pub fn greet(self: &Self) {
        io.println(self.name);
    }
}

fn main() void {
    const person = Person.new("Alice", 30);
    person.greet();
}
```

### Interfaces

```lang
pub interface Printable {
    fn print();
    fn getValue() i32;
}

pub struct Widget {
    value: i32,

    pub fn new(value: i32) Widget {
        return Widget{ value };
    }

    pub fn print(self: &Self) {
        io.println(self.value);
    }

    pub fn getValue(self: &Self) i32 {
        return self.value;
    }
}
```

### Enums

```lang
pub enum<T> Status {
    Todo,
    WIP,
    Done,
    Error(T),
};

fn main() void {
    const s = Status.Todo;
    const e = Status.Error("something wrong");
}
```

### Error Types

```lang
pub error FileError {
    NotFound(String),
    PermissionDenied,
}

error UnionError = FileError | StatusError;

fn readFile() i64! {
    return 1;
}

fn main() i64! {
    const result = try readFile();
    
    // Catch error with block
    const handled = readFile() catch |e| {
        io.println("error occurred");
        0
    };
    
    // Catch error with expression
    const defaultVal = readFile() catch |_| 10;
    
    return result + handled + defaultVal;
}
```

### Optional Types

```lang
fn main() void {
    const g: ?i32 = 432;
    const z = if (g) |v| v else 10;
    
    const h: ?i32 = null;
    const w = if (h) |v| v * 2 else {
        var c = 3;
        c + 1
    };
}
```

### Switch Statement

```lang
fn main() void {
    const value = 42;

    switch (value) {
        1 => io.println("one"),
        42 => io.println("answer"),
        100..200 => io.println("range"),
        _ => io.println("default"),
    }

    // Multiple cases
    switch (value) {
        1, 2, 3 => io.println("one two three"),
        _ => io.println("other"),
    }

    // Character cases
    const c = 'a';
    switch (c) {
        'a'..'z' => io.println("lowercase"),
        'A'..'Z' => io.println("uppercase"),
        _ => io.println("other"),
    }

    // Enum with pattern capture
    const status = Status.Todo;
    switch (status) {
        Status.Todo => io.println("todo"),
        Status.Error => |e| io.println("error"),
        _ => io.println("other"),
    }
}
```

### For Loop

```lang
fn main() void {
    // Range-based (exclusive end)
    for (1..10) |v| {
        io.println(v);
    }

    // Array iteration with index and value
    const arr = [3]u8{1, 2, 3};
    for (arr) |index, value| {
        io.println(index, value);
    }

    // Ignore index or value
    for (arr) |_, v| { io.println(v); }
    for (arr) |i, _| { io.println(i); }

    // Iterator-based (with custom struct)
    var f = Iter { i: 0 };
    for (f.next()) |e| {
        io.println(e);
    }

    // Infinite loop
    for {
        io.println("once");
        break;
    }

    // Conditional loop
    for (true) {
        io.println("runs once");
        break;
    }

    // Labeled loop with break
    outer: for (1..10) |v| {
        for (1..10) |b| {
            break outer;
        }
    }
}

struct Iter {
    i: u8,

    pub fn next(self: *Self) ?u8 {
        if (self.i < 10) {
            const t = self.i;
            self.i += 1;
            return t;
        }
        return null;
    }
}
```

### Arrays

```lang
fn main() void {
    const arr1: [3]u8 = {1, 3, 4};
    var arr2 = [3]u8{1, 2, 3};
    arr2[0] = 5;
}
```

### Tuples

```lang
fn main() void {
    const t: (i64, i64, i64) = (1, 2, 3);
    const first = t.0;
    const second = t.1;
    const third = t.2;
    
    // Destructuring
    const (a, b, c) = t;
    io.println(a);
}
```

### Raw Pointers (FFI)

```lang
// Using rawptr for general FFI
external cdecl {
    fn malloc(size: u64) rawptr;
    fn free(ptr: rawptr) void;
}

fn main() void {
    const ptr = malloc(16);
    if (@is_not_null(ptr)) {
        io.println("allocated memory");
    }
    free(ptr);
}

// Using pointer types for C interop
struct CString {
    data: *const u8,
}

external cdecl {
    fn puts(message: *const u8) i32;
}

fn main2() i32 {
    const msg: *const u8 = "Hello from C!";
    return puts(msg);
}
```

### Defer

```lang
fn main() void {
    const file = open_file("test.txt");
    defer close_file(file);
    defer! cleanup(file); // propagates error
    io.println("working with file");
}

fn open_file(path: String) i64! {
    return 1;
}

fn close_file(fd: i64) void {
    io.println("closed");
}

fn cleanup(fd: i64) i64! {
    return 0;
}
```

### Generics

```lang
struct Compose<T> {
    value: T,
    
    pub fn new(value: T) Compose<T> {
        return Compose{ value };
    }
    
    pub fn get(self: &Self) T {
        return self.value;
    }
}

fn add<T>(a: T, b: T) T {
    return a;
}

fn main() void {
    const c = Compose.new(42);
    const val = c.get();
    io.println(val);
}
```

### Operators

```lang
fn main() void {
    // Arithmetic
    const a: i32 = 1 + 3;
    const b: i32 = 1 - 3;
    const c: i32 = 1 * 3;
    const d: i32 = 1 / 3;
    const e: i32 = 1 % 3;

    // Bitwise
    const f: i32 = 1 << 3;
    const g: i32 = 1 >> 3;
    const h: i32 = 1 & 3;
    const i: i32 = 1 | 3;
    const j: i32 = 1 ^ 3;

    // Comparison
    const k: bool = 1 == 3;
    const l: bool = 1 != 3;
    const m: bool = 1 < 3;
    const n: bool = 1 > 3;
    const o: bool = 1 <= 3;
    const p: bool = 1 >= 3;

    // Logical
    const q: bool = true && false;
    const r: bool = true || false;
    const s: bool = !true;
}
```

### Testing

```lang
import "testing"

fn main() i64 {
    testing.assert(true, "This should pass");
    testing.assert_eq_i64(1, 1, "1 should equal 1");
    testing.assert_ne_i64(1, 2, "1 should not equal 2");
    testing.assert_eq_bool(true, true, "true should equal true");
    testing.assert_eq_string("hello", "hello", "strings should match");
    return 0;
}
```

## Building from Source

### Prerequisites

- **Rust** (2024 edition)
- **LLVM 21** (for macOS)
- **Clang** (for final linking)

### Build Instructions

```bash
# Clone the repository
git clone https://github.com/septemlee/lang.git
cd lang

# Build the compiler
cargo build --release

# Run the binary
./target/release/lang --help
```

## Usage

### Commands

| Command | Description |
|---------|-------------|
| `lang run <file>` | Run a Lang source file via JIT |
| `lang build <file> -o <output>` | Build to native executable |
| `lang jit <file>` | Run via JIT compiler |
| `lang ir <file>` | Generate LLVM IR only |

### Examples

```bash
# Run a simple program
lang run examples/test_simple.lang

# Build to executable
lang build examples/test_features.lang -o myapp

# Generate LLVM IR
lang ir examples/test_struct.lang
```

## Standard Library

### io

```lang
import "io"

fn main() void {
    io.println("Print with newline");
    io.print("Print without newline");
}
```

### math

```lang
import "math"

fn main() void {
    const abs_val = math.abs(-5);
    const max_val = math.max(10, 20);
    const min_val = math.min(10, 20);
}
```

### testing

```lang
import "testing"

fn main() i64 {
    testing.assert(true, "Test message");
    testing.assert_eq_i64(1, 1, "Values should be equal");
    return 0;
}
```

## Project Structure

```
lang/
├── src/
│   ├── ast.rs         # Abstract Syntax Tree definitions
│   ├── codegen.rs     # LLVM IR code generation
│   ├── grammar.pest   # Parser grammar definition
│   ├── hir.rs         # High-level IR (HIR) definitions
│   ├── lexer.rs       # Tokenization
│   ├── lower.rs       # AST to HIR lowering
│   ├── main.rs        # Compiler entry point
│   ├── opt.rs         # HIR optimizer
│   ├── parser.rs      # Parsing logic
│   ├── sema.rs        # Semantic analyzer
│   └── stdlib.rs      # Standard library loader
├── std/
│   ├── io.lang        # I/O standard library
│   ├── math.lang      # Math standard library
│   ├── http.lang      # HTTP standard library
│   └── testing.lang   # Testing library
├── examples/          # Example programs
└── Cargo.toml         # Project manifest
```

## Architecture

```
Source Code (.lang)
       │
       ▼
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│    Lexer    │────▶│   Parser    │────▶│     AST     │
└─────────────┘     └─────────────┘     └─────────────┘
                                              │
                                              ▼
                                        ┌─────────────┐
                                        │    Sema     │
                                        │  (Semantic  │
                                        │  Analysis)  │
                                        └─────────────┘
                                              │
                                              ▼
                                        ┌─────────────┐
                                        │  Lowering   │
                                        │    (HIR)    │
                                        └─────────────┘
                                              │
                                              ▼
                                        ┌─────────────┐
                                        │  Optimizer  │
                                        └─────────────┘
                                              │
                                              ▼
                                        ┌─────────────┐
                                        │   Codegen   │
                                        │  (LLVM IR)  │
                                        └─────────────┘
                                              │
                      ┌─────────────┐         │
                      │    Clang    │◀────────┘
                      │   (Linker)  │
                      └─────────────┘
                            │
                            ▼
                      ┌─────────────┐
                      │ Executable  │
                      └─────────────┘
```

### Compilation Pipeline

1. **Lexer** (`src/lexer.rs`) - Tokenizes source code into tokens
2. **Parser** (`src/parser.rs`) - Parses tokens into AST (Abstract Syntax Tree)
3. **Semantic Analyzer** (`src/sema.rs`) - Type checking and symbol resolution
4. **Lowering** (`src/lower.rs`) - Transforms AST to HIR (High-level IR)
5. **Optimizer** (`src/opt.rs`) - Optimizes HIR
6. **Code Generator** (`src/codegen.rs`) - Generates LLVM IR from HIR
7. **Clang/LLVM** - Compiles LLVM IR to native executable

## Roadmap

- [ ] Interface Type Support
- [ ] Interface Generic Support
- [ ] Generic Constraints (Methods)
- [ ] Type Definitions
