//! JIT command - runs Lang source file via JIT compiler

use std::error::Error;

use super::run::run_jit;

/// Run via JIT compiler
pub fn run_jit_command(
    source: &str,
    cli_std_path: Option<std::path::PathBuf>,
) -> Result<(), Box<dyn Error>> {
    run_jit(source, cli_std_path)
}
