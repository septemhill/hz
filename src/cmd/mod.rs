//! CLI Commands for the Lang compiler
//!
//! This module contains the implementation of each CLI subcommand.

pub mod ast;
pub mod build;
pub mod hir;
pub mod ir;
pub mod jit;
pub mod lsp;
pub mod run;
pub mod typelist;

pub use ast::dump_ast;
pub use build::build;
pub use hir::dump_hir;
pub use ir::generate_ir;
#[allow(unused_imports)]
pub use jit::run_jit_command;
pub use lsp::run_lsp;
pub use run::run_jit;
pub use typelist::run_typelist_command;

use std::path::PathBuf;

/// Resolve the standard library path from CLI argument, environment variable, or fallback
pub fn resolve_std_path(cli_std_path: Option<PathBuf>) -> PathBuf {
    if let Some(path) = cli_std_path {
        return path;
    }
    if let Ok(env_path) = std::env::var("LANG_STD_PATH") {
        return PathBuf::from(env_path);
    }
    let local_std = PathBuf::from("./std");
    if local_std.exists() {
        return local_std;
    }
    PathBuf::from("/usr/local/lib/lang/std")
}
