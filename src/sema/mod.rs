// Semantic analysis module
// Provides symbol table management and various analysis passes

pub mod error;
pub mod global;
pub mod infer;
pub mod intrinsics;
pub mod mutability;
pub mod resolver;
pub mod symbol;
pub mod treeshaker;
pub mod typelist;

use crate::ast::{ErrorDef, ErrorVariant, FnDef, InterfaceDef, Type};
use std::collections::{HashMap, HashSet};

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
#[allow(unused)]
pub use treeshaker::{TreeShaker, TreeShakerStats, treeshake};

// ============================================================================
// Main Semantic Analyzer
// Orchestrates all analysis passes
// ============================================================================

pub struct SemanticAnalyzer {
    pub symbol_table: SymbolTable,
    pub typed_program: Option<TypedProgram>,
    pub structs: HashMap<String, crate::ast::StructDef>,
    pub interfaces: HashMap<String, crate::ast::InterfaceDef>,
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
            interfaces: HashMap::new(),
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
            interfaces: HashMap::new(),
            enums: HashMap::new(),
            errors: HashMap::new(),
            functions: HashMap::new(),
            std_path,
        }
    }

    /// Analyze with optional stdlib packages for import resolution
    pub fn analyze_with_stdlib(
        &mut self,
        program: &mut crate::ast::Program,
        stdlib: Option<&crate::stdlib::StdLib>,
        enable_tree_shaking: bool,
    ) -> AnalysisResult<()> {
        normalize_interfaces(&mut program.interfaces)?;
        normalize_error_unions(&mut program.errors)?;

        // If stdlib is provided, pre-populate symbol table with imported functions
        if let Some(stdlib) = stdlib {
            // Collect all functions from imported packages
            for (alias, package_name) in &program.imports {
                // Resolve alias to actual package name
                let ns = if let Some(a) = alias {
                    a.clone()
                } else {
                    // Extract the last part of the package name for the namespace
                    // e.g., "utils/sub" -> "sub"
                    package_name
                        .split('/')
                        .last()
                        .unwrap_or(package_name.as_str())
                        .to_string()
                };

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
            eprintln!(
                "DEBUG mod.rs registering struct: {} with {} fields",
                s.name,
                s.fields.len()
            );
            self.structs.insert(s.name.clone(), s.clone());
            // Register methods in global functions map for monomorphization
            for method in &s.methods {
                let mut m = method.clone();
                let mut combined_params = s.generic_params.clone();
                combined_params.extend(m.generic_params.clone());
                m.generic_params = combined_params;

                // Replace "Self" with the struct type (including generic params)
                let generic_args: Vec<Type> = s
                    .generic_params
                    .iter()
                    .map(|p| Type::GenericParam(p.clone()))
                    .collect();
                eprintln!(
                    "DEBUG mod.rs replace_self for struct={}, generic_args={:?}",
                    s.name, generic_args
                );
                for p in &mut m.params {
                    p.ty.replace_self_with_args(&s.name, &generic_args);
                }
                m.return_ty.replace_self_with_args(&s.name, &generic_args);

                self.functions.insert(format!("{}_{}", s.name, m.name), m);
            }
        }
        for i in &program.interfaces {
            self.interfaces.insert(i.name.clone(), i.clone());
        }
        for e in &program.enums {
            self.enums.insert(e.name.clone(), e.clone());
            // Register methods in global functions map for monomorphization
            for method in &e.methods {
                let mut m = method.clone();
                let mut combined_params = e.generic_params.clone();
                combined_params.extend(m.generic_params.clone());
                m.generic_params = combined_params;

                // Replace "Self" with the enum type (including generic params)
                let generic_args: Vec<Type> = e
                    .generic_params
                    .iter()
                    .map(|p| Type::GenericParam(p.clone()))
                    .collect();
                for p in &mut m.params {
                    p.ty.replace_self_with_args(&e.name, &generic_args);
                }
                m.return_ty.replace_self_with_args(&e.name, &generic_args);

                self.functions.insert(format!("{}_{}", e.name, m.name), m);
            }
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

        // Pass 2: Symbol resolution - Move this BEFORE type inference
        // Resolves variable/function references and checks scope correctness
        let symbol_table_for_resolver = self.symbol_table.clone();
        let mut symbol_resolver = SymbolResolver::new(
            symbol_table_for_resolver,
            self.structs.clone(),
            self.enums.clone(),
            program.imports.clone(),
        );
        symbol_resolver.analyze(program)?;

        // Pass 3: Type inference - produce type-annotated AST
        // Use the symbol table from symbol_resolver which contains resolved local variables
        let symbol_table_after_resolver = symbol_resolver.get_symbol_table().clone();
        let typed_prog = infer_types(
            program,
            symbol_table_after_resolver.clone(),
            self.structs.clone(),
            self.interfaces.clone(),
            self.enums.clone(),
            self.errors.clone(),
            self.functions.clone(),
        )?;
        self.typed_program = Some(typed_prog.clone());

        // Pass 4: Mutability analysis
        // Use the final symbol table from typed_program
        let mut mutability_analyzer = MutabilityAnalyzer::new(
            symbol_table_after_resolver,
            typed_prog
        );
        mutability_analyzer.analyze(program)?;

        // Store final symbol table
        self.symbol_table = mutability_analyzer.get_symbol_table().clone();

        // Pass 6: Tree-shaking - remove dead code (optional)
        if enable_tree_shaking {
            // We need a mutable reference for treeshaking, but the function signature uses &Program
            // So we use a separate scope with explicit mutable borrow
            let final_symbol_table = self.symbol_table.clone();
            let stats = crate::sema::treeshake(program, final_symbol_table);
            eprintln!(
                "TreeShaker: reachable functions={}, structs={}, enums={}",
                stats.reachable_functions, stats.reachable_structs, stats.reachable_enums
            );
        }

        Ok(())
    }

    /// Analyze program without stdlib (for backward compatibility)
    pub fn analyze(&mut self, program: &mut crate::ast::Program) -> AnalysisResult<()> {
        self.analyze_with_stdlib(program, None, true)
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

fn normalize_interfaces(interfaces: &mut [InterfaceDef]) -> AnalysisResult<()> {
    let interface_map: HashMap<String, InterfaceDef> = interfaces
        .iter()
        .cloned()
        .map(|interface| (interface.name.clone(), interface))
        .collect();
    let mut expanded_cache: HashMap<String, Vec<FnDef>> = HashMap::new();

    for interface in interfaces.iter_mut() {
        let mut visiting = HashSet::new();
        interface.methods = expand_interface_methods(
            &interface.name,
            &interface_map,
            &mut expanded_cache,
            &mut visiting,
        )?;
    }

    Ok(())
}

fn expand_interface_methods(
    interface_name: &str,
    interface_map: &HashMap<String, InterfaceDef>,
    expanded_cache: &mut HashMap<String, Vec<FnDef>>,
    visiting: &mut HashSet<String>,
) -> AnalysisResult<Vec<FnDef>> {
    if let Some(methods) = expanded_cache.get(interface_name) {
        return Ok(methods.clone());
    }

    if !visiting.insert(interface_name.to_string()) {
        return Err(AnalysisError::new(&format!(
            "Cyclic interface composition involving '{}'",
            interface_name
        ))
        .with_module("sema"));
    }

    let interface_def = interface_map.get(interface_name).ok_or_else(|| {
        AnalysisError::new(&format!("Unknown interface '{}'", interface_name)).with_module("sema")
    })?;

    let mut expanded_methods = Vec::new();
    for composed_name in &interface_def.composed_interfaces {
        let composed_methods =
            expand_interface_methods(composed_name, interface_map, expanded_cache, visiting)?;
        for method in composed_methods {
            merge_interface_method(&mut expanded_methods, method, interface_name)?;
        }
    }

    for method in &interface_def.methods {
        merge_interface_method(&mut expanded_methods, method.clone(), interface_name)?;
    }

    visiting.remove(interface_name);
    expanded_cache.insert(interface_name.to_string(), expanded_methods.clone());
    Ok(expanded_methods)
}

fn merge_interface_method(
    expanded_methods: &mut Vec<FnDef>,
    method: FnDef,
    interface_name: &str,
) -> AnalysisResult<()> {
    if let Some(existing) = expanded_methods
        .iter()
        .find(|existing| existing.name == method.name)
    {
        if !interface_methods_equal(existing, &method) {
            return Err(AnalysisError::new(&format!(
                "Conflicting interface method '{}' while composing '{}'",
                method.name, interface_name
            ))
            .with_module("sema"));
        }
        return Ok(());
    }

    expanded_methods.push(method);
    Ok(())
}

fn interface_methods_equal(left: &FnDef, right: &FnDef) -> bool {
    left.name == right.name
        && left.return_ty == right.return_ty
        && left
            .params
            .iter()
            .map(|param| &param.ty)
            .eq(right.params.iter().map(|param| &param.ty))
        && left.generic_params == right.generic_params
        && left.generic_constraints == right.generic_constraints
}

fn normalize_error_unions(errors: &mut [ErrorDef]) -> AnalysisResult<()> {
    let error_map: HashMap<String, ErrorDef> = errors
        .iter()
        .cloned()
        .map(|error| (error.name.clone(), error))
        .collect();
    let mut expanded_cache: HashMap<String, Vec<ErrorVariant>> = HashMap::new();

    for error in errors.iter_mut() {
        let mut visiting = HashSet::new();
        error.variants =
            expand_error_variants(&error.name, &error_map, &mut expanded_cache, &mut visiting)?;
    }

    Ok(())
}

fn expand_error_variants(
    error_name: &str,
    error_map: &HashMap<String, ErrorDef>,
    expanded_cache: &mut HashMap<String, Vec<ErrorVariant>>,
    visiting: &mut HashSet<String>,
) -> AnalysisResult<Vec<ErrorVariant>> {
    if let Some(variants) = expanded_cache.get(error_name) {
        return Ok(variants.clone());
    }

    if !visiting.insert(error_name.to_string()) {
        return Err(
            AnalysisError::new(&format!("Cyclic error union involving '{}'", error_name))
                .with_module("sema"),
        );
    }

    let error_def = error_map.get(error_name).ok_or_else(|| {
        AnalysisError::new(&format!("Unknown error type '{}'", error_name)).with_module("sema")
    })?;

    let mut expanded_variants = Vec::new();
    let mut seen_variants = HashSet::new();

    for variant in &error_def.variants {
        let included_error = if variant.associated_types.is_empty() {
            error_map.get(&variant.name)
        } else {
            None
        };

        if let Some(included_error) = included_error {
            let nested_variants =
                expand_error_variants(&included_error.name, error_map, expanded_cache, visiting)?;

            for nested_variant in nested_variants {
                if seen_variants.insert(nested_variant.name.clone()) {
                    expanded_variants.push(nested_variant);
                }
            }
        } else if seen_variants.insert(variant.name.clone()) {
            expanded_variants.push(variant.clone());
        }
    }

    visiting.remove(error_name);
    expanded_cache.insert(error_name.to_string(), expanded_variants.clone());
    Ok(expanded_variants)
}
