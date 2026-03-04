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

- **Strong Type System**: Integer types (i8-i64, u8-u64), booleans, void, and optional types
- **Optional Types**: Nullable types using `?` prefix (e.g., `?i32`, `?bool`)
- **Structs**: User-defined data types with methods
- **Interfaces**: Polymorphism through trait-like interfaces
- **Enums**: Algebraic data types with generic parameters
- **Arrays**: Fixed-size arrays (e.g., `[3]u8`)
- **Functions**: First-class functions with typed parameters and return values
- **Control Flow**: `if/else`, `while`, `loop`, `for` (range and iterator), `switch`
- **Method Syntax**: Object-oriented programming with `self` parameter
- **Modules**: Import system for code organization
- **Visibility**: `pub` keyword for public functions and fields

## Language Syntax

### Hello World

```lang
import "io"

fn main() {
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

fn main() {
    const person = Person.new("Alice", 30);
    person.greet();
}
```

### Enums

```lang
enum Status<T> {
    Todo,
    WIP,
    Done,
    Error(T),
};

fn main() {
    const s = Status.Todo;
    const e = Status.Error("something wrong");
}
```

### Switch Statement

```lang
fn main() {
    const value = 42;

    switch (value) {
        1 => io.println("one"),
        42 => io.println("answer"),
        _ => io.println("default"),
    }
}
```

### For Loop

```lang
fn main() {
    // Range-based
    for (i range 1..10) {
        io.println(i);
    }

    // Array iteration
    const arr = [3]u8{1, 2, 3};
    for (item range arr) {
        io.println(item);
    }
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

fn main() {
    io.println("Print with newline");
    io.print("Print without newline");
}
```

### math

```lang
import "math"

fn main() {
    const abs_val = math.abs(-5);
    const max_val = math.max(10, 20);
    const min_val = math.min(10, 20);
}
```

## Project Structure

```
lang/
├── src/
│   ├── ast.rs          # Abstract Syntax Tree definitions
│   ├── codegen.rs      # LLVM IR code generation
│   ├── grammar.pest    # Parser grammar definition
│   ├── lexer.rs        # Tokenization
│   ├── main.rs         # Compiler entry point
│   ├── parser.rs       # Parsing logic
│   └── stdlib.rs       # Standard library loader
├── std/
│   ├── io.lang         # I/O standard library
│   ├── math.lang       # Math standard library
│   └── http.lang       # HTTP standard library
├── examples/           # Example programs
└── Cargo.toml          # Project manifest
```

## Architecture

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│   Lexer     │────▶│   Parser    │────▶│     AST     │
└─────────────┘     └─────────────┘     └─────────────┘
                                              │
                                              ▼
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│  Executable │◀────│    Clang    │◀────│   LLVM IR   │
└─────────────┘     └─────────────┘     └─────────────┘
                                              │
                                              ▼
                                        ┌─────────────┐
                                        │  Codegen    │
                                        └─────────────┘
```

## Roadmap

- [ ] Interface Type Support
- [ ] Interface Generic Support
- [ ] Generic Constraints (Methods)
- [ ] Type Definitions

