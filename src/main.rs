//! # Lang Programming Language Compiler
//!
//! A system programming language targeting macOS with LLVM backend.

mod ast;
mod build;
mod builtin;
mod codegen;
mod hir;
mod lexer;
mod lower;
mod opt;
mod parser;
mod sema;
mod stdlib;

#[cfg(feature = "lsp")]
mod lsp;

mod cmd;

use std::fs;

use clap::Parser;

/// CLI arguments for the Lang compiler
#[derive(clap::Parser, Debug)]
#[command(name = "lang")]
#[command(version = "0.1.0")]
#[command(about = "Lang Programming Language Compiler", long_about = None)]
struct Cli {
    /// Standard library path
    #[arg(long = "std", value_name = "PATH", global = true)]
    std_path: Option<std::path::PathBuf>,

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
    /// Run LSP server
    Lsp {
        /// Enable verbose logging
        #[arg(short = 'v', long = "verbose")]
        verbose: bool,
    },
    /// Generate LLVM IR only
    Ir {
        /// Source file to generate IR from
        #[arg(value_name = "FILE")]
        source: std::path::PathBuf,
        /// Output file for IR (optional)
        #[arg(short = 'o', long = "output", value_name = "OUTPUT")]
        output: Option<std::path::PathBuf>,
        /// Disable tree-shaking
        #[arg(long = "no-tree-shaking")]
        no_tree_shaking: bool,
    },
    /// Dump HIR (High-level Intermediate Representation)
    Hir {
        /// Source file to dump HIR from
        #[arg(value_name = "FILE")]
        source: std::path::PathBuf,
        /// Output file for HIR (optional)
        #[arg(short = 'o', long = "output", value_name = "OUTPUT")]
        output: Option<std::path::PathBuf>,
        /// Disable tree-shaking
        #[arg(long = "no-tree-shaking")]
        no_tree_shaking: bool,
    },
    /// Dump AST (Abstract Syntax Tree)
    Ast {
        /// Source file to dump AST from
        #[arg(value_name = "FILE")]
        source: std::path::PathBuf,
        /// Disable tree-shaking
        #[arg(long = "no-tree-shaking")]
        no_tree_shaking: bool,
    },
    /// List all types with their unique IDs
    Typelist {
        /// Source file to list types from
        #[arg(value_name = "FILE")]
        source: std::path::PathBuf,
    },
    /// Initialize a new Lang project
    Init {
        /// Project name
        #[arg(value_name = "NAME")]
        name: String,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();

    // Parse CLI arguments
    let cli = Cli::parse();

    match cli.command {
        Commands::Run { source } => {
            let source_content = fs::read_to_string(&source)?;
            cmd::run_jit(&source_content, cli.std_path)?;
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
            cmd::build(&source, &output_path, &include, cli.std_path)?;
        }
        Commands::Jit { source } => {
            let source_content = fs::read_to_string(&source)?;
            cmd::jit::run_jit_command(&source_content, cli.std_path)?;
        }
        Commands::Ir {
            source,
            output,
            no_tree_shaking,
        } => {
            let source_content = fs::read_to_string(&source)?;
            let output_path = output.map(|p| p.to_string_lossy().to_string());
            cmd::generate_ir(&source_content, output_path, cli.std_path, !no_tree_shaking)?;
        }
        Commands::Hir {
            source,
            output,
            no_tree_shaking,
        } => {
            let source_content = fs::read_to_string(&source)?;
            let output_path = output.map(|p| p.to_string_lossy().to_string());
            cmd::dump_hir(&source_content, output_path, cli.std_path, !no_tree_shaking)?;
        }
        Commands::Ast {
            source,
            no_tree_shaking,
        } => {
            let source_content = fs::read_to_string(&source)?;
            cmd::dump_ast(&source_content, cli.std_path, !no_tree_shaking)?;
        }
        Commands::Lsp { verbose } => {
            cmd::run_lsp(verbose, cli.std_path);
        }
        Commands::Typelist { source } => {
            let source_content = fs::read_to_string(&source)?;
            cmd::run_typelist_command(&source_content, cli.std_path)?;
        }
        Commands::Init { name } => {
            cmd::init_project(&name)?;
        }
    }

    Ok(())
}
