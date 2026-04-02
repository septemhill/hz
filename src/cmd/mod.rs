//! CLI Commands for the Lang compiler
//!
//! This module contains the implementation of each CLI subcommand.

pub mod ast;
pub mod build;
pub mod env;
pub mod hir;
pub mod init;
pub mod ir;
pub mod jit;
pub mod lsp;
pub mod run;
pub mod typelist;

pub use ast::dump_ast;
pub use build::build;
pub use hir::dump_hir;
pub use init::init_project;
pub use ir::generate_ir;
#[allow(unused_imports)]
pub use jit::run_jit_command;
pub use lsp::run_lsp;
pub use run::run_jit;
pub use typelist::run_typelist_command;

use std::path::PathBuf;

use self::env::CompilerEnv;

/// Resolve the standard library path from CLI argument, environment variable, or fallback
pub fn resolve_std_path(cli_std_path: Option<PathBuf>) -> PathBuf {
    if let Some(path) = cli_std_path {
        return path;
    }
    CompilerEnv::new()
        .get("STD_LIB_PATH")
        .cloned()
        .unwrap_or_else(|| PathBuf::from("/usr/local/lib/lang/std"))
}

/// Print all compiler environment variables
pub fn print_env() {
    CompilerEnv::new().print();
}

/// Print a specific compiler environment variable by key
/// Returns true if the key was found, false otherwise
pub fn print_env_key(key: &str) -> bool {
    CompilerEnv::new().print_key(key)
}
