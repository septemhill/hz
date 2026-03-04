//! # Lang Programming Language Compiler
//!
//! A system programming language targeting macOS with LLVM backend.

use std::error::Error;
use std::fs;

mod ast;
mod codegen;
mod lexer;
mod parser;
mod stdlib;

use clap::Parser;

/// CLI arguments for the Lang compiler
#[derive(clap::Parser, Debug)]
#[command(name = "lang")]
#[command(version = "0.1.0")]
#[command(about = "Lang Programming Language Compiler", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand, Debug)]
enum Commands {
    /// Run a Lang source file
    Run {
        /// Source file to run
        #[arg(value_name = "FILE")]
        source: std::path::PathBuf,
    },
    /// Build a Lang source file to executable
    Build {
        /// Source file to build
        #[arg(value_name = "FILE")]
        source: std::path::PathBuf,
        /// Output file path
        #[arg(short = 'o', long = "output", value_name = "OUTPUT")]
        output: Option<std::path::PathBuf>,
    },
    /// Run via JIT compiler
    Jit {
        /// Source file to run
        #[arg(value_name = "FILE")]
        source: std::path::PathBuf,
    },
    /// Generate LLVM IR only
    Ir {
        /// Source file to generate IR from
        #[arg(value_name = "FILE")]
        source: std::path::PathBuf,
        /// Output file for IR (optional)
        #[arg(short = 'o', long = "output", value_name = "OUTPUT")]
        output: Option<std::path::PathBuf>,
    },
}

/// Compile source code to executable
fn compile(source: &str, output_path: &str) -> Result<(), Box<dyn Error>> {
    // Step 0: Initialize std library
    println!("[0/5] Loading std library...");
    let mut stdlib = stdlib::StdLib::new();
    // Set std path to ./std relative to current directory
    stdlib.set_std_path("./std");
    // Don't preload packages - require explicit imports
    // let _ = stdlib.preload_common();
    println!(
        "    Loaded std packages: {:?}",
        stdlib.packages().keys().collect::<Vec<_>>()
    );

    // Step 1: Parse source code into AST
    println!("[1/5] Parsing source code...");
    let program = parser::parse(source)?;
    println!("    Found {} function(s)", program.functions.len());
    for func in &program.functions {
        println!("    - {}", func.name);
    }

    // Step 2: Generate LLVM IR
    println!("[2/5] Generating LLVM IR...");
    let context = inkwell::context::Context::create();
    let mut codegen = codegen::CodeGenerator::new(&context, "lang", stdlib)?;
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
    println!("[3/5] Writing LLVM IR to file...");
    let ir_path = format!("{}.ll", output_path);
    fs::write(&ir_path, &ir)?;
    println!("    Written to {}", ir_path);

    // Step 4: Compile to object file
    println!("[4/5] Compiling to object file...");
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
                return Err(format!("clang compilation failed: {}", stderr).into());
            }
        }
        Err(e) => {
            return Err(format!("Could not run clang: {}", e).into());
        }
    }

    // Step 5: Link to create executable
    println!("[5/5] Linking to create executable...");
    let exec_path = output_path.to_string();

    // Check if main function exists
    let has_main = program.functions.iter().any(|f| f.name == "main");
    if !has_main {
        println!("    Warning: No main function found, creating executable anyway");
    }

    // Use clang to link the object file
    let link_result = std::process::Command::new("clang")
        .args(&["-o", &exec_path, &obj_path])
        .output();

    match link_result {
        Ok(output) => {
            if output.status.success() {
                println!("    Linked to {}", exec_path);
                println!("    Executable ready: {}", exec_path);
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(format!("clang linking failed: {}", stderr).into());
            }
        }
        Err(e) => {
            return Err(format!("Could not run clang for linking: {}", e).into());
        }
    }

    Ok(())
}

/// Run the compiled program (JIT)
fn run_jit(source: &str) -> Result<(), Box<dyn Error>> {
    // Initialize std library
    println!("Loading std library...");
    let mut stdlib = stdlib::StdLib::new();
    // Set std path to ./std relative to current directory
    stdlib.set_std_path("./std");
    // Don't preload packages - require explicit imports
    // let _ = stdlib.preload_common();
    println!(
        "Loaded std packages: {:?}",
        stdlib.packages().keys().collect::<Vec<_>>()
    );

    // Parse source code
    println!("Parsing source code...");
    let program = parser::parse(source)?;

    // Generate LLVM IR
    let context = inkwell::context::Context::create();
    let mut codegen = codegen::CodeGenerator::new(&context, "lang", stdlib)?;
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

/// Generate LLVM IR only
fn generate_ir(source: &str, output_path: Option<String>) -> Result<(), Box<dyn Error>> {
    // Initialize std library
    println!("Loading std library...");
    let mut stdlib = stdlib::StdLib::new();
    stdlib.set_std_path("./std");
    println!(
        "Loaded std packages: {:?}",
        stdlib.packages().keys().collect::<Vec<_>>()
    );

    // Parse source code
    println!("Parsing source code...");
    let program = parser::parse(source)?;

    // Generate LLVM IR
    let context = inkwell::context::Context::create();
    let mut codegen = codegen::CodeGenerator::new(&context, "lang", stdlib)?;
    codegen.generate(&program)?;
    let ir = codegen.print_ir();

    // Output IR
    if let Some(ref path) = output_path {
        std::fs::write(path, &ir)?;
        println!("LLVM IR written to {}", path);
    } else {
        println!("{}", ir);
    }

    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    // Initialize logging
    env_logger::init();

    // Parse CLI arguments
    let cli = Cli::parse();

    match cli.command {
        Commands::Run { source } => {
            let source_content = fs::read_to_string(&source)?;
            run_jit(&source_content)?;
        }
        Commands::Build { source, output } => {
            let source_content = fs::read_to_string(&source)?;
            let output_path = output
                .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().to_string()))
                .unwrap_or_else(|| "a".to_string());
            compile(&source_content, &output_path)?;
        }
        Commands::Jit { source } => {
            let source_content = fs::read_to_string(&source)?;
            run_jit(&source_content)?;
        }
        Commands::Ir { source, output } => {
            let source_content = fs::read_to_string(&source)?;
            let output_path = output.map(|p| p.to_string_lossy().to_string());
            generate_ir(&source_content, output_path)?;
        }
    }

    Ok(())
}
