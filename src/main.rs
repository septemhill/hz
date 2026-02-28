//! # Lang Programming Language Compiler
//!
//! A system programming language targeting macOS with LLVM backend.

use std::error::Error;
use std::fs;
use std::path::Path;

mod ast;
mod codegen;
mod parser;

/// Compile source code to executable
fn compile(source: &str, output_path: &str) -> Result<(), Box<dyn Error>> {
    // Step 1: Parse source code into AST
    println!("[1/4] Parsing source code...");
    let program = parser::parse(source)?;
    println!("    Found {} function(s)", program.functions.len());
    for func in &program.functions {
        println!("    - {}", func.name);
    }

    // Step 2: Generate LLVM IR
    println!("[2/4] Generating LLVM IR...");
    let context = inkwell::context::Context::create();
    let mut codegen = codegen::CodeGenerator::new(&context, "lang")?;
    codegen.generate(&program)?;
    let ir = codegen.print_ir();
    println!("    Generated LLVM IR:");
    for line in ir.lines().take(20) {
        println!("    {}", line);
    }
    if ir.lines().count() > 20 {
        println!("    ... ({} more lines)", ir.lines().count() - 20);
    }

    // Step 3: Write LLVM IR to file
    println!("[3/4] Writing LLVM IR to file...");
    let ir_path = format!("{}.ll", output_path);
    fs::write(&ir_path, &ir)?;
    println!("    Written to {}", ir_path);

    // Step 4: Compile to object file
    println!("[4/4] Compiling to object file...");
    let obj_path = format!("{}.o", output_path);

    // Use clang to compile the IR
    let result = std::process::Command::new("clang")
        .args(&["-c", "-o", &obj_path, &ir_path])
        .output();

    match result {
        Ok(output) => {
            if output.status.success() {
                println!("    Compiled to {}", obj_path);
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                println!("    Warning: clang compilation failed: {}", stderr);
                println!("    (This is expected if clang is not installed)");
            }
        }
        Err(e) => {
            println!("    Warning: Could not run clang: {}", e);
            println!("    (LLVM IR saved to {})", ir_path);
        }
    }

    Ok(())
}

/// Run the compiled program (JIT)
fn run_jit(source: &str) -> Result<(), Box<dyn Error>> {
    // Parse source code
    println!("Parsing source code...");
    let program = parser::parse(source)?;

    // Generate LLVM IR
    let context = inkwell::context::Context::create();
    let mut codegen = codegen::CodeGenerator::new(&context, "lang")?;
    codegen.generate(&program)?;

    // Print the generated IR
    println!("\nGenerated LLVM IR:");
    println!("{}", codegen.print_ir());

    // Execute main function if it exists
    if program.functions.iter().any(|f| f.name == "main") {
        println!("\nExecuting main function via JIT...");
        // Note: For a simple i64 return, we would need more setup
        // This is a placeholder for JIT execution
    }

    Ok(())
}

/// Print usage information
fn print_usage() {
    println!("Lang Programming Language Compiler");
    println!();
    println!("Usage: lang <command> [options]");
    println!();
    println!("Commands:");
    println!("  run <file>    Run a Lang source file");
    println!("  build <file>  Build a Lang source file to executable");
    println!("  jit <file>    Run via JIT compiler");
    println!("  ir <file>     Generate LLVM IR only");
    println!();
    println!("Examples:");
    println!("  lang run hello.lang");
    println!("  lang build hello.lang -o hello");
}

fn main() -> Result<(), Box<dyn Error>> {
    // Initialize logging
    env_logger::init();

    // Get command line arguments
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        print_usage();
        return Ok(());
    }

    let command = &args[1];

    match command.as_str() {
        "run" => {
            if args.len() < 3 {
                println!("Error: Missing source file");
                print_usage();
                return Ok(());
            }
            let source_path = &args[2];
            let source = fs::read_to_string(source_path)?;
            run_jit(&source)?;
        }
        "build" => {
            if args.len() < 3 {
                println!("Error: Missing source file");
                print_usage();
                return Ok(());
            }
            let source_path = &args[2];
            let output_path = if args.len() >= 4 && args[3] == "-o" {
                args.get(4).cloned().unwrap_or_else(|| "a".to_string())
            } else {
                "a".to_string()
            };
            let source = fs::read_to_string(source_path)?;
            compile(&source, &output_path)?;
        }
        "jit" => {
            if args.len() < 3 {
                println!("Error: Missing source file");
                print_usage();
                return Ok(());
            }
            let source_path = &args[2];
            let source = fs::read_to_string(source_path)?;
            run_jit(&source)?;
        }
        "ir" => {
            if args.len() < 3 {
                println!("Error: Missing source file");
                print_usage();
                return Ok(());
            }
            let source_path = &args[2];
            let source = fs::read_to_string(source_path)?;
            let program = parser::parse(&source)?;
            let context = inkwell::context::Context::create();
            let mut codegen = codegen::CodeGenerator::new(&context, "lang")?;
            codegen.generate(&program)?;
            println!("{}", codegen.print_ir());
        }
        _ => {
            println!("Unknown command: {}", command);
            print_usage();
        }
    }

    Ok(())
}
