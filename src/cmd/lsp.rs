//! LSP command - runs the Lang Language Server Protocol server

/// Run LSP server
#[cfg(feature = "lsp")]
pub fn run_lsp(verbose: bool, _cli_std_path: Option<std::path::PathBuf>) {
    crate::lsp::run_lsp_server();
}

/// Run LSP server (non-LSP build)
#[cfg(not(feature = "lsp"))]
pub fn run_lsp(_verbose: bool, _cli_std_path: Option<std::path::PathBuf>) {
    println!("LSP server not compiled. Enable the 'lsp' feature: cargo build --features lsp");
}
