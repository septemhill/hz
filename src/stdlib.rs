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
    pub structs: Vec<StructDef>,
    pub enums: Vec<EnumDef>,
}

/// Standard library package manager
pub struct StdLib {
    packages: HashMap<String, Package>,
    std_path: Option<String>,
}

impl StdLib {
    /// Create a new std library manager
    pub fn new() -> Self {
        StdLib {
            packages: HashMap::new(),
            std_path: None,
        }
    }

    /// Set the std library path
    pub fn set_std_path(&mut self, path: &str) {
        self.std_path = Some(path.to_string());
    }

    /// Load a package from the std library
    pub fn load_package(&mut self, name: &str) -> Result<Package, String> {
        // Check if already loaded
        if let Some(pkg) = self.packages.get(name) {
            return Ok(pkg.clone());
        }

        // Find the package file
        let std_path = self.std_path.as_ref().ok_or("Std path not set")?;

        let package_path = format!("{}/{}.lang", std_path, name);

        let source = fs::read_to_string(&package_path)
            .map_err(|e| format!("Failed to load package '{}': {}", name, e))?;

        // Parse the package
        let program = parser::parse(&source)
            .map_err(|e| format!("Failed to parse package '{}': {}", name, e))?;

        let package = Package {
            name: name.to_string(),
            functions: program.functions,
            structs: program.structs,
            enums: program.enums,
        };

        // Cache the package
        self.packages.insert(name.to_string(), package.clone());

        Ok(package)
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
