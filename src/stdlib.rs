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

        Err(format!(
            "Package '{}' not found in search paths: {:?}",
            name, self.search_paths
        ))
    }

    /// Get a function from a package
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
}

impl Default for StdLib {
    fn default() -> Self {
        Self::new()
    }
}

/// Try to resolve a namespace to a package and function
pub fn resolve_namespace_call(stdlib: &StdLib, namespace: &str, function: &str) -> Option<FnDef> {
    stdlib.get_function(namespace, function)
}
