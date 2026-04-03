//! # Build Orchestrator for Multi-file Projects
//!
//! This module manages the compilation of projects with multiple source files.

use std::collections::{HashSet, VecDeque};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use inkwell::targets::{
    CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine,
};

use crate::ast;
use crate::codegen;
use crate::lower;
use crate::opt;
use crate::parser;
use crate::sema;
use crate::stdlib;

/// Represents a single compilation unit (a .lang file)
pub struct CompilationUnit {
    pub path: PathBuf,
    pub name: String,
    pub package_name: Option<String>,
    #[allow(unused)]
    pub imports: Vec<String>,
}

pub struct BuildSystem {
    std_path: PathBuf,
    search_paths: Vec<PathBuf>,
    units: Vec<CompilationUnit>,
}

impl BuildSystem {
    /// Create a new build system
    pub fn new(std_path: PathBuf) -> Self {
        let mut search_paths = Vec::new();
        search_paths.push(std_path.clone());
        BuildSystem {
            std_path,
            search_paths,
            units: Vec::new(),
        }
    }

    /// Add a search path
    pub fn add_search_path(&mut self, path: PathBuf) {
        if !self.search_paths.contains(&path) {
            self.search_paths.push(path);
        }
    }

    /// Discover all dependencies starting from an entry point
    pub fn discover_dependencies(&mut self, entry_path: &Path) -> Result<(), Box<dyn Error>> {
        let mut queue = VecDeque::new();
        let mut visited = HashSet::new();
        let mut file_to_package = std::collections::HashMap::new();

        // First pass: collect all files using BFS
        let entry_abs = fs::canonicalize(entry_path)?;
        queue.push_back((entry_abs.clone(), None));

        while let Some((current_path, pkg_name)) = queue.pop_front() {
            if visited.contains(&current_path) {
                continue;
            }

            if current_path.is_dir() {
                // If it's a directory, add all .lang files in it to the queue
                // The pkg_name here is the name used in the import statement
                for entry in fs::read_dir(&current_path)? {
                    let entry = entry?;
                    let path = entry.path();
                    if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("lang") {
                        let abs_path = fs::canonicalize(path)?;
                        if !visited.contains(&abs_path) {
                            queue.push_back((abs_path, pkg_name.clone()));
                        }
                    }
                }
                visited.insert(current_path);
                continue;
            }

            visited.insert(current_path.clone());
            if let Some(name) = pkg_name {
                file_to_package.insert(current_path.clone(), name);
            }

            let source = fs::read_to_string(&current_path)?;
            let program = parser::parse(&source)?;

            // Add all dependencies to queue
            for (_, package_name) in &program.imports {
                // Try to find if this is a local file or directory
                if let Some(local_path) = self.resolve_local_import(&current_path, package_name) {
                    if !visited.contains(&local_path) {
                        queue.push_back((local_path, Some(package_name.clone())));
                    }
                }
            }
        }

        // Second pass: build ordered list (dependencies first)
        let mut ordered: Vec<PathBuf> = visited.into_iter().filter(|p| p.is_file()).collect();
        ordered.sort();
        ordered.reverse();

        // Now create CompilationUnits in the correct order
        for path in ordered {
            let source = fs::read_to_string(&path)?;
            let program = parser::parse(&source)?;

            let unit_name = path.file_stem().unwrap().to_string_lossy().to_string();
            let package_name = file_to_package.get(&path).cloned();
            let imports: Vec<String> = program
                .imports
                .iter()
                .map(|(_, name)| name.clone())
                .collect();

            self.units.push(CompilationUnit {
                path,
                name: unit_name,
                package_name,
                imports,
            });
        }

        Ok(())
    }

    /// Resolve a local import relative to another file
    fn resolve_local_import(&self, from_file: &Path, package_name: &str) -> Option<PathBuf> {
        let parent = from_file.parent().unwrap();

        // 1. Check for a directory (new package system)
        let package_dir = parent.join(package_name);
        if package_dir.is_dir() {
            return Some(fs::canonicalize(package_dir).unwrap());
        }

        // 2. Fallback: Check for a .lang file
        let local_path = parent.join(format!("{}.lang", package_name));
        if local_path.exists() {
            return Some(fs::canonicalize(local_path).unwrap());
        }

        // Also check search paths
        for path in &self.search_paths {
            // Check for directory in search path
            let p_dir = path.join(package_name);
            if p_dir.is_dir() {
                return Some(fs::canonicalize(p_dir).unwrap());
            }

            // Check for file in search path
            let p_file = path.join(format!("{}.lang", package_name));
            if p_file.exists() {
                return Some(fs::canonicalize(p_file).unwrap());
            }
        }

        None
    }

    /// Build the project
    pub fn build(&mut self, entry_path: &Path, output_path: &str) -> Result<(), Box<dyn Error>> {
        println!("Discovering dependencies...");
        self.discover_dependencies(entry_path)?;

        let mut object_files = Vec::new();

        for unit in &self.units {
            println!("Compiling {}...", unit.name);
            let obj_file = self.compile_unit(unit)?;
            object_files.push(obj_file);
        }

        println!("Linking executable...");
        self.link(object_files, output_path)?;

        Ok(())
    }

    /// Compile a single unit to an object file
    fn compile_unit(&self, unit: &CompilationUnit) -> Result<PathBuf, Box<dyn Error>> {
        let source = fs::read_to_string(&unit.path)?;
        let mut stdlib = stdlib::StdLib::new();
        stdlib.set_std_path(self.std_path.to_str().unwrap());

        // Add project directory to search paths
        if let Some(parent) = unit.path.parent() {
            let parent_str: &str = parent.to_str().expect("Valid parent path");
            stdlib.add_search_path(parent_str);
        }
        for path in &self.search_paths {
            let path_str: &str = path.to_str().expect("Valid search path string");
            stdlib.add_search_path(path_str);
        }

        // 1. Parse
        let mut program = parser::parse(&source)?;

        // Load imports into stdlib
        for (_, package_name) in &program.imports {
            let _ = stdlib.load_package(package_name);
        }

        // 2. Sema
        let mut analyzer = sema::SemanticAnalyzer::new();
        analyzer
            .analyze_with_stdlib(&mut program, Some(&stdlib), true)
            .map_err(|e| {
                let file_name = unit.path.to_str().unwrap_or("unknown");
                // Calculate line number from span offset
                let line_num = e.line.map(|offset| {
                    source[..offset.min(source.len())]
                        .chars()
                        .filter(|&c| c == '\n')
                        .count()
                        + 1
                });
                let mut err = e.with_file(file_name);
                if let Some(line) = line_num {
                    err = err.with_line(line);
                }
                format!("{}", err)
            })?;

        // 3. Lower
        let mut lowering_ctx = lower::LoweringContext::new();
        lowering_ctx.set_symbol_table(analyzer.get_symbol_table().clone());
        let typed_program = analyzer
            .get_typed_program()
            .ok_or("No typed program found")?;
        let mut hir_program = lowering_ctx.lower_program(&program, typed_program);

        // 4. Opt
        opt::optimize(&mut hir_program);

        // 5. Codegen
        let context = inkwell::context::Context::create();
        let typed_program = analyzer
            .get_typed_program()
            .ok_or("No typed program found")?;
        let mut monomorphized_structs = std::collections::HashMap::new();
        for s in &typed_program.structs {
            monomorphized_structs.insert(s.name.clone(), s.clone());
        }

        let codegen_module_name = if let Some(pkg) = &unit.package_name {
            // Use the last component of the package name as the module name (namespace)
            // e.g., "utils/sub" -> "sub"
            pkg.split('/').last().unwrap_or(pkg.as_str()).to_string()
        } else {
            unit.name.clone()
        };

        let mut codegen = codegen::CodeGenerator::new(
            &context,
            &codegen_module_name,
            stdlib,
            monomorphized_structs,
            analyzer.enums.clone(),
            analyzer.errors.clone(),
        )?;

        codegen.process_imports(&program.imports)?;

        // Note: stdlib function bodies are NOT generated here.
        // They need to be compiled separately and linked, OR we need to
        // parse and compile them as part of the main program.
        // For now, we rely on the functions being declared as external.

        // Declarations
        for s in &typed_program.structs {
            codegen.declare_struct(s)?;
        }
        for e in analyzer.enums.values() {
            codegen.declare_enum(e)?;
        }
        for f in &typed_program.functions {
            codegen.declare_function(f)?;
        }
        // Declare external C functions (FFI)
        for ext_fn in &program.external_functions {
            codegen.declare_c_function(ext_fn)?;
        }

        // Generate
        codegen.generate_hir(&hir_program)?;
        let ir = codegen.print_ir();

        // Write IR for inspection, but emit the object file via LLVM directly so we
        // don't depend on the system clang understanding our LLVM textual IR syntax.
        let ir_path = unit.path.with_extension("ll");
        let obj_path = unit.path.with_extension("o");
        fs::write(&ir_path, &ir)?;

        Target::initialize_native(&InitializationConfig::default())?;
        let triple = TargetMachine::get_default_triple();
        codegen.module.set_triple(&triple);

        let target = Target::from_triple(&triple)?;
        let target_machine = target
            .create_target_machine(
                &triple,
                "generic",
                "",
                inkwell::OptimizationLevel::Default,
                RelocMode::Default,
                CodeModel::Default,
            )
            .ok_or("failed to create LLVM target machine")?;

        let data_layout = target_machine.get_target_data().get_data_layout();
        codegen.module.set_data_layout(&data_layout);
        target_machine.write_to_file(&codegen.module, FileType::Object, &obj_path)?;

        Ok(obj_path)
    }

    /// Link object files into an executable
    fn link(&self, object_files: Vec<PathBuf>, output_path: &str) -> Result<(), Box<dyn Error>> {
        let mut args = Vec::new();
        args.push("-o".to_string());
        args.push(output_path.to_string());
        for obj in &object_files {
            args.push(obj.to_str().unwrap().to_string());
        }

        // Link with libc (required for FFI external functions)
        args.push("-lc".to_string());

        println!("Link command: clang {}", args.join(" "));

        let result = std::process::Command::new("clang").args(&args).output()?;

        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr);
            return Err(format!("clang linking failed: {}", stderr).into());
        }

        Ok(())
    }
}
