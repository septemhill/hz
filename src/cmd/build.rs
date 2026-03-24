//! Build command - compiles Lang source file to executable

use std::error::Error;
use std::path::Path;

/// Compile source code to executable (Multi-file enabled)
pub fn build(
    source_path: &Path,
    output_path: &str,
    include_paths: &[std::path::PathBuf],
    cli_std_path: Option<std::path::PathBuf>,
) -> Result<(), Box<dyn Error>> {
    let stdlib_path = crate::cmd::resolve_std_path(cli_std_path);

    let mut build_system = crate::build::BuildSystem::new(stdlib_path);
    for path in include_paths {
        build_system.add_search_path(path.clone());
    }

    build_system.build(source_path, output_path)
}
