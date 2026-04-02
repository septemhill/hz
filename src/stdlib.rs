//! # Standard Library Package Manager
//!
//! This module handles loading and managing std library packages.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::ast::*;
use crate::parser;

/// Represents a loaded package
#[derive(Debug, Clone)]
#[allow(unused)]
pub struct Package {
    pub name: String,
    pub functions: Vec<FnDef>,
    pub external_functions: Vec<ExternalFnDef>,
    pub structs: Vec<StructDef>,
    pub enums: Vec<EnumDef>,
}

/// Package loader and manager
pub struct StdLib {
    packages: HashMap<String, Package>,
    std_path: Option<String>,
    search_paths: Vec<String>,
}

impl StdLib {
    #[allow(unused)]
    /// Create a new package manager
    pub fn new() -> Self {
        StdLib {
            packages: HashMap::new(),
            std_path: None,
            search_paths: Vec::new(),
        }
    }

    /// Set the std library path
    pub fn set_std_path(&mut self, path: &str) {
        self.std_path = Some(path.to_string());
        // Also add it to search paths
        self.add_search_path(path);
    }

    /// Add a search path for packages
    pub fn add_search_path(&mut self, path: &str) {
        if !self.search_paths.contains(&path.to_string()) {
            self.search_paths.push(path.to_string());
        }
    }

    /// Load a package by name, searching through search paths
    pub fn load_package(&mut self, name: &str) -> Result<Package, String> {
        // Check if already loaded
        if let Some(pkg) = self.packages.get(name) {
            return Ok(pkg.clone());
        }

        // Search through all search paths
        for path in &self.search_paths {
            let base_path = Path::new(path);
            let package_dir = base_path.join(name);

            if package_dir.is_dir() {
                // It's a directory package - load all .lang files in this directory (but not subdirectories)
                let mut combined_package = Package {
                    name: name.to_string(),
                    functions: Vec::new(),
                    external_functions: Vec::new(),
                    structs: Vec::new(),
                    enums: Vec::new(),
                };

                let entries = fs::read_dir(&package_dir).map_err(|e| {
                    format!(
                        "Failed to read package directory '{}': {}",
                        package_dir.display(),
                        e
                    )
                })?;

                for entry in entries {
                    let entry = entry.map_err(|e| e.to_string())?;
                    let file_path = entry.path();

                    // Only include .lang files, ignore directories and other file types
                    if file_path.is_file()
                        && file_path.extension().and_then(|s| s.to_str()) == Some("lang")
                    {
                        let source = fs::read_to_string(&file_path).map_err(|e| {
                            format!(
                                "Failed to read file '{}' in package: {}",
                                file_path.display(),
                                e
                            )
                        })?;

                        // Parse the file
                        let program = parser::parse(&source).map_err(|e| {
                            format!(
                                "Failed to parse file '{}' in package: {}",
                                file_path.display(),
                                e
                            )
                        })?;

                        // Merge into combined package
                        combined_package.functions.extend(program.functions);
                        combined_package
                            .external_functions
                            .extend(program.external_functions);
                        combined_package.structs.extend(program.structs);
                        combined_package.enums.extend(program.enums);
                    }
                }

                // Cache and return the combined package
                self.packages
                    .insert(name.to_string(), combined_package.clone());
                return Ok(combined_package);
            } else {
                // Fallback: check for a single file package (name.lang)
                let package_path = if path.ends_with(".lang") {
                    // If the path itself is a file, check if its name matches
                    let path_obj = Path::new(path);
                    if path_obj.file_stem().and_then(|s| s.to_str()) == Some(name) {
                        path.to_string()
                    } else {
                        continue;
                    }
                } else {
                    format!("{}/{}.lang", path, name)
                };

                if Path::new(&package_path).exists() {
                    let source = fs::read_to_string(&package_path).map_err(|e| {
                        format!(
                            "Failed to read package '{}' at {}: {}",
                            name, package_path, e
                        )
                    })?;

                    // Parse the package
                    let program = parser::parse(&source).map_err(|e| {
                        format!(
                            "Failed to parse package '{}' at {}: {}",
                            name, package_path, e
                        )
                    })?;

                    let package = Package {
                        name: name.to_string(),
                        functions: program.functions,
                        external_functions: program.external_functions,
                        structs: program.structs,
                        enums: program.enums,
                    };

                    // Cache the package
                    self.packages.insert(name.to_string(), package.clone());
                    return Ok(package);
                }
            }
        }

        Err(format!(
            "Package '{}' not found in search paths: {:?}",
            name, self.search_paths
        ))
    }

    /// Get a function from a package
    #[allow(unused)]
    pub fn get_function(&self, package_name: &str, fn_name: &str) -> Option<FnDef> {
        self.packages
            .get(package_name)
            .and_then(|pkg| pkg.functions.iter().find(|f| f.name == fn_name).cloned())
    }

    /// Get all packages
    pub fn packages(&self) -> &HashMap<String, Package> {
        &self.packages
    }

    /// Preload common std packages
    #[allow(unused)]
    pub fn preload_common(&mut self) -> Result<(), String> {
        // Try to load common packages if std path is set
        if self.std_path.is_some() {
            // Try to load io package
            match self.load_package("io") {
                Ok(_) => println!("    Loaded 'io' package successfully"),
                Err(e) => println!("    Warning: Failed to load 'io' package: {}", e),
            }
        } else {
            println!("    Warning: Std path not set, skipping package preload");
        }
        Ok(())
    }

    /// Preload builtin package (contains built-in functions like @is_null, @is_not_null)
    #[allow(unused)]
    pub fn preload_builtins(&mut self) -> Result<(), String> {
        // Try to load builtin package if std path is set
        if self.std_path.is_some() {
            match self.load_package("builtin") {
                Ok(_) => println!("    Loaded 'builtin' package successfully"),
                Err(e) => println!("    Warning: Failed to load 'builtin' package: {}", e),
            }
        } else {
            println!("    Warning: Std path not set, skipping builtin preload");
        }
        Ok(())
    }
}

impl Default for StdLib {
    fn default() -> Self {
        Self::new()
    }
}

/// Try to resolve a namespace to a package and function
#[allow(unused)]
pub fn resolve_namespace_call(stdlib: &StdLib, namespace: &str, function: &str) -> Option<FnDef> {
    stdlib.get_function(namespace, function)
}
