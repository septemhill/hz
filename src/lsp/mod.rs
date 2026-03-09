//! # Lang Language Server Protocol Implementation
//!
//! This module provides LSP support for the Lang programming language.

#[cfg(feature = "lsp")]
pub mod handlers;
#[cfg(feature = "lsp")]
pub mod semantic_tokens;
#[cfg(feature = "lsp")]
pub mod server;
#[cfg(feature = "lsp")]
pub mod state;

#[cfg(not(feature = "lsp"))]
pub mod stubs {
    //! Stub implementations when LSP feature is not enabled

    /// Stub for running LSP server
    pub fn run_lsp_server() {
        println!("LSP server not compiled. Enable the 'lsp' feature.");
    }
}

#[cfg(feature = "lsp")]
pub use server::run_lsp_server;
#[cfg(not(feature = "lsp"))]
pub use stubs::run_lsp_server;
