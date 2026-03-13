//! Build command - compiles Lang source file to executable

use std::error::Error;
use std::path::Path;

/// Compile source code to executable (Multi-file enabled)
pub fn build(
    source_path: &Path,
    output_path: &str,
    include_paths: &[std::path::PathBuf],
) -> Result<(), Box<dyn Error>> {
    let mut stdlib_path = std::path::PathBuf::from("./std");
    if !stdlib_path.exists() {
        // Fallback or handle error
        stdlib_path = std::path::PathBuf::from("/usr/local/lib/lang/std");
    }

    let mut build_system = crate::build::BuildSystem::new(stdlib_path);
    for path in include_paths {
        build_system.add_search_path(path.clone());
    }

    build_system.build(source_path, output_path)
}
