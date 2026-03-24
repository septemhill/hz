// Semantic analysis module
// Provides symbol table management and various analysis passes

pub mod error;
pub mod global;
pub mod infer;
pub mod mutability;
pub mod resolver;
pub mod symbol;
pub mod types;

use std::collections::HashMap;

#[cfg(test)]
mod tests;

// Re-export for convenience
#[allow(unused_imports)]
pub use error::{AnalysisError, AnalysisResult};
pub use global::GlobalDefinitionsAnalyzer;
pub use infer::{TypedProgram, infer_types};
pub use mutability::MutabilityAnalyzer;
pub use resolver::SymbolResolver;
#[allow(unused_imports)]
pub use symbol::{Scope, Symbol, SymbolTable};
pub use types::TypeAnalyzer;

// ============================================================================
// Main Semantic Analyzer
// Orchestrates all analysis passes
// ============================================================================

pub struct SemanticAnalyzer {
    pub symbol_table: SymbolTable,
    pub typed_program: Option<TypedProgram>,
    pub structs: HashMap<String, crate::ast::StructDef>,
    pub enums: HashMap<String, crate::ast::EnumDef>,
    pub errors: HashMap<String, crate::ast::ErrorDef>,
    pub functions: HashMap<String, crate::ast::FnDef>,
    pub std_path: String,
}

impl SemanticAnalyzer {
    pub fn new() -> Self {
        SemanticAnalyzer {
            symbol_table: SymbolTable::new(),
            typed_program: None,
            structs: HashMap::new(),
            enums: HashMap::new(),
            errors: HashMap::new(),
            functions: HashMap::new(),
            std_path: String::from("std"),
        }
    }

    pub fn with_std_path(std_path: String) -> Self {
        SemanticAnalyzer {
            symbol_table: SymbolTable::new(),
            typed_program: None,
            structs: HashMap::new(),
            enums: HashMap::new(),
            errors: HashMap::new(),
            functions: HashMap::new(),
            std_path,
        }
    }

    /// Analyze with optional stdlib packages for import resolution
    pub fn analyze_with_stdlib(
        &mut self,
        program: &crate::ast::Program,
        stdlib: Option<&crate::stdlib::StdLib>,
    ) -> AnalysisResult<()> {
        // If stdlib is provided, pre-populate symbol table with imported functions
        if let Some(stdlib) = stdlib {
            // Collect all functions from imported packages
            for (alias, package_name) in &program.imports {
                // Resolve alias to actual package name
                let ns = alias.as_deref().unwrap_or(package_name);

                // Try to get the package from stdlib
                if let Some(pkg) = stdlib.packages().get(package_name) {
                    // Add public functions from the package
                    for fn_def in &pkg.functions {
                        if fn_def.visibility != crate::ast::Visibility::Public {
                            continue;
                        }

                        // Create mangled name: namespace_function
                        let mangled_name = format!("{}_{}", ns, fn_def.name);
                        self.symbol_table.define(
                            mangled_name,
                            crate::ast::Type::Function {
                                params: fn_def.params.iter().map(|p| p.ty.clone()).collect(),
                                return_type: Box::new(fn_def.return_ty.clone()),
                            },
                            fn_def.visibility,
                            true, // const
                        );
                    }

                    // Add public external functions
                    for ext_fn in &pkg.external_functions {
                        if ext_fn.visibility != crate::ast::Visibility::Public {
                            continue;
                        }
                        let mangled_name = format!("{}_{}", ns, ext_fn.name);
                        self.symbol_table.define(
                            mangled_name,
                            ext_fn.return_ty.clone(),
                            ext_fn.visibility,
                            true,
                        );
                    }

                    // Add public structs
                    for s in &pkg.structs {
                        if s.visibility != crate::ast::Visibility::Public {
                            continue;
                        }
                        let mangled_name = format!("{}_{}", ns, s.name);

                        // Add to structs map so TypeInferrer can find fields/methods
                        let mut mangled_s = s.clone();
                        mangled_s.name = mangled_name.clone();
                        self.structs.insert(mangled_name.clone(), mangled_s);

                        self.symbol_table.define(
                            mangled_name.clone(),
                            crate::ast::Type::Custom {
                                name: mangled_name.clone(),
                                generic_args: vec![],
                                is_exported: true,
                            },
                            s.visibility,
                            true,
                        );

                        // Also register struct methods
                        for method in &s.methods {
                            if method.visibility != crate::ast::Visibility::Public {
                                continue;
                            }
                            let method_mangled = format!("{}_{}", mangled_name, method.name);
                            self.symbol_table.define(
                                method_mangled,
                                crate::ast::Type::Function {
                                    params: method.params.iter().map(|p| p.ty.clone()).collect(),
                                    return_type: Box::new(method.return_ty.clone()),
                                },
                                method.visibility,
                                true,
                            );
                        }
                    }

                    // Add public enums
                    for e in &pkg.enums {
                        if e.visibility != crate::ast::Visibility::Public {
                            continue;
                        }
                        let mangled_name = format!("{}_{}", ns, e.name);

                        // Add to enums map
                        let mut mangled_e = e.clone();
                        mangled_e.name = mangled_name.clone();
                        self.enums.insert(mangled_name.clone(), mangled_e);

                        self.symbol_table.define(
                            mangled_name.clone(),
                            crate::ast::Type::Custom {
                                name: mangled_name.clone(),
                                generic_args: vec![],
                                is_exported: true,
                            },
                            e.visibility,
                            true,
                        );

                        // Register enum variants
                        for variant in &e.variants {
                            let variant_full_name = format!("{}.{}", mangled_name, variant.name);
                            self.symbol_table.define(
                                variant_full_name,
                                crate::ast::Type::Custom {
                                    name: mangled_name.clone(),
                                    generic_args: vec![],
                                    is_exported: true,
                                },
                                crate::ast::Visibility::Public,
                                true,
                            );
                        }

                        // Register enum methods
                        for method in &e.methods {
                            if method.visibility != crate::ast::Visibility::Public {
                                continue;
                            }
                            let method_mangled = format!("{}_{}", mangled_name, method.name);
                            self.symbol_table.define(
                                method_mangled,
                                crate::ast::Type::Function {
                                    params: method.params.iter().map(|p| p.ty.clone()).collect(),
                                    return_type: Box::new(method.return_ty.clone()),
                                },
                                method.visibility,
                                true,
                            );
                        }
                    }
                }
            }
        }

        // Populate local definitions into our maps
        for s in &program.structs {
            self.structs.insert(s.name.clone(), s.clone());
        }
        for e in &program.enums {
            self.enums.insert(e.name.clone(), e.clone());
        }
        for err in &program.errors {
            self.errors.insert(err.name.clone(), err.clone());
        }
        for f in &program.functions {
            self.functions.insert(f.name.clone(), f.clone());
        }

        // Pass 1: Collect and validate global definitions
        let mut global_analyzer = GlobalDefinitionsAnalyzer::new();
        global_analyzer.analyze(program)?;

        // Merge global definitions into our symbol table
        self.symbol_table
            .merge(global_analyzer.get_symbol_table().clone());

        // Pass 2: Type inference - produce type-annotated AST (must run early for function types)
        let symbol_table = self.symbol_table.clone();
        let typed_prog = infer_types(
            program,
            symbol_table.clone(),
            self.structs.clone(),
            self.enums.clone(),
            self.errors.clone(),
            self.functions.clone(),
        )?;
        self.typed_program = Some(typed_prog.clone());

        // Pass 3: Type analysis - use the SAME symbol table that was passed to infer_types
        let mut type_analyzer = TypeAnalyzer::new(
            symbol_table,
            self.structs.clone(),
            self.enums.clone(),
            self.errors.clone(),
        );
        type_analyzer.analyze(program)?;

        // Pass 4: Symbol resolution
        let symbol_table = type_analyzer.get_symbol_table().clone();
        let mut symbol_resolver = SymbolResolver::new(
            symbol_table,
            self.structs.clone(),
            self.enums.clone(),
        );
        symbol_resolver.analyze(program)?;

        // Pass 5: Mutability analysis
        let symbol_table = symbol_resolver.get_symbol_table().clone();
        let mut mutability_analyzer = MutabilityAnalyzer::new(symbol_table, typed_prog);
        mutability_analyzer.analyze(program)?;

        // Store final symbol table
        self.symbol_table = mutability_analyzer.get_symbol_table().clone();

        Ok(())
    }

    /// Analyze program without stdlib (for backward compatibility)
    pub fn analyze(&mut self, program: &crate::ast::Program) -> AnalysisResult<()> {
        self.analyze_with_stdlib(program, None)
    }

    #[allow(unused)]
    pub fn get_symbol_table(&self) -> &SymbolTable {
        &self.symbol_table
    }

    pub fn get_typed_program(&self) -> Option<&TypedProgram> {
        self.typed_program.as_ref()
    }
}

impl Default for SemanticAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}
