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

pub use ast::dump_ast;
pub use build::build;
pub use hir::dump_hir;
pub use ir::generate_ir;
#[allow(unused_imports)]
pub use jit::run_jit_command;
pub use lsp::run_lsp;

// Re-export run_jit from run module for use by Run and Jit commands
pub use run::run_jit;
