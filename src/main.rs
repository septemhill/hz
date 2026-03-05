//! # Lang Programming Language Compiler
//!
//! A system programming language targeting macOS with LLVM backend.

use std::error::Error;
use std::fs;

mod ast;
mod build;
mod codegen;
mod hir;
mod lexer;
mod lower;
mod opt;
mod parser;
mod sema;
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
        /// Include search paths
        #[arg(short = 'I', long = "include", value_name = "PATH")]
        include: Vec<std::path::PathBuf>,
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
    /// Dump HIR (High-level Intermediate Representation)
    Hir {
        /// Source file to dump HIR from
        #[arg(value_name = "FILE")]
        source: std::path::PathBuf,
        /// Output file for HIR (optional)
        #[arg(short = 'o', long = "output", value_name = "OUTPUT")]
        output: Option<std::path::PathBuf>,
    },
}

/// Compile source code to executable (Multi-file enabled)
fn compile(
    source_path: &std::path::Path,
    output_path: &str,
    include_paths: &[std::path::PathBuf],
) -> Result<(), Box<dyn Error>> {
    let mut stdlib_path = std::path::PathBuf::from("./std");
    if !stdlib_path.exists() {
        // Fallback or handle error
        stdlib_path = std::path::PathBuf::from("/usr/local/lib/lang/std");
    }

    let mut build_system = build::BuildSystem::new(stdlib_path);
    for path in include_paths {
        build_system.add_search_path(path.clone());
    }

    build_system.build(source_path, output_path)
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

    let mut lowering_ctx = lower::LoweringContext::new();
    let hir_program = lowering_ctx.lower_program(&program);

    for f in &program.functions {
        codegen.declare_function(f)?;
    }
    codegen.generate_hir(&hir_program)?;

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

    // Parse source code
    println!("Parsing source code...");
    let program = parser::parse(source)?;

    // Load imported packages
    println!("Loading imported packages...");
    for (_, package_name) in &program.imports {
        stdlib.load_package(package_name)?;
    }
    println!(
        "Loaded std packages: {:?}",
        stdlib.packages().keys().collect::<Vec<_>>()
    );

    // Generate LLVM IR
    let context = inkwell::context::Context::create();
    let mut codegen = codegen::CodeGenerator::new(&context, "lang", stdlib)?;

    // Process imports (declares functions from imported packages)
    codegen.process_imports(&program.imports)?;

    // Declare structs and enums
    for s in &program.structs {
        codegen.declare_struct(s)?;
    }
    for e in &program.enums {
        codegen.declare_enum(e)?;
    }

    // Declare functions
    for f in &program.functions {
        codegen.declare_function(f)?;
    }

    // Declare external C functions (FFI)
    for ext_fn in &program.external_functions {
        codegen.declare_c_function(ext_fn)?;
    }

    // Lower and generate
    let mut lowering_ctx = lower::LoweringContext::new();
    let hir_program = lowering_ctx.lower_program(&program);
    codegen.generate_hir(&hir_program)?;
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

/// Dump HIR (High-level Intermediate Representation)
fn dump_hir(source: &str, output_path: Option<String>) -> Result<(), Box<dyn Error>> {
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
    println!("    Found {} function(s)", program.functions.len());

    // Semantic Analysis
    println!("Semantic Analysis...");
    let mut analyzer = sema::SemanticAnalyzer::new();
    analyzer.analyze(&program)?;

    // Lower to HIR
    println!("Lowering to HIR...");
    let mut lowering_ctx = lower::LoweringContext::new();
    let hir_program = lowering_ctx.lower_program(&program);

    // Format HIR using Debug
    let hir_debug = format!("{:?}", hir_program);

    // Output HIR
    if let Some(ref path) = output_path {
        std::fs::write(path, &hir_debug)?;
        println!("HIR written to {}", path);
    } else {
        println!("{}", hir_debug);
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
        Commands::Build {
            source,
            output,
            include,
        } => {
            let output_path = output
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| {
                    // Use the source file stem as the output name
                    source
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| "a".to_string())
                });
            compile(&source, &output_path, &include)?;
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
        Commands::Hir { source, output } => {
            let source_content = fs::read_to_string(&source)?;
            let output_path = output.map(|p| p.to_string_lossy().to_string());
            dump_hir(&source_content, output_path)?;
        }
    }

    Ok(())
}
