use crate::ast::Type;
use crate::sema::error::{AnalysisError, AnalysisResult};
use crate::sema::symbol::SymbolTable;
use std::collections::HashMap;

// List of builtin functions that cannot be overridden
const BUILTIN_FUNCTIONS: &[&str] = &[
    "@is_null",
    "@is_not_null",
    "@size_of",
    "@align_of",
    "@type_of",
];

// ============================================================================
// Analysis Pass 1: Global Definitions Analyzer
// Collects and validates all global symbols (functions, external functions,
// structs, enums, errors)
// ============================================================================

pub struct GlobalDefinitionsAnalyzer {
    symbol_table: SymbolTable,
}

impl GlobalDefinitionsAnalyzer {
    pub fn new() -> Self {
        GlobalDefinitionsAnalyzer {
            symbol_table: SymbolTable::new(),
        }
    }

    pub fn analyze(&mut self, program: &crate::ast::Program) -> AnalysisResult<SymbolTable> {
        self.collect_functions(&program.functions)?;
        self.collect_external_functions(&program.external_functions)?;
        self.collect_interfaces(&program.interfaces)?;
        self.collect_structs(&program.structs)?;
        self.collect_enums(&program.enums)?;
        self.collect_errors(&program.errors)?;
        self.validate_interface_impls(program)?;
        Ok(self.symbol_table.clone())
    }

    fn collect_functions(&mut self, functions: &[crate::ast::FnDef]) -> AnalysisResult<()> {
        for f in functions {
            // Check if this function conflicts with a builtin function
            if BUILTIN_FUNCTIONS.contains(&f.name.as_str()) {
                return Err(AnalysisError::new_with_span(
                    &format!("Cannot override builtin function '{}'", f.name),
                    &f.span,
                )
                .with_module("global"));
            }
            if self.symbol_table.resolve(&f.name).is_some() {
                return Err(AnalysisError::new_with_span(
                    &format!("Duplicate declaration of function '{}'", f.name),
                    &f.span,
                )
                .with_module("global"));
            }
            self.symbol_table.define_with_generics(
                f.name.clone(),
                Type::Function {
                    params: f.params.iter().map(|p| p.ty.clone()).collect(),
                    return_type: Box::new(f.return_ty.clone()),
                },
                f.visibility,
                true,
                f.generic_params.clone(),
                None,
            );
        }
        Ok(())
    }

    fn collect_external_functions(
        &mut self,
        ext_fns: &[crate::ast::ExternalFnDef],
    ) -> AnalysisResult<()> {
        for ext_fn in ext_fns {
            // Check if this external function conflicts with a builtin function
            if BUILTIN_FUNCTIONS.contains(&ext_fn.name.as_str()) {
                return Err(AnalysisError::new_with_span(
                    &format!("Cannot override builtin function '{}'", ext_fn.name),
                    &ext_fn.span,
                )
                .with_module("global"));
            }
            if self.symbol_table.resolve(&ext_fn.name).is_some() {
                return Err(AnalysisError::new_with_span(
                    &format!(
                        "Duplicate declaration of external function '{}'",
                        ext_fn.name
                    ),
                    &ext_fn.span,
                )
                .with_module("global"));
            }
            self.symbol_table.define(
                ext_fn.name.clone(),
                Type::Function {
                    params: ext_fn.params.iter().map(|p| p.ty.clone()).collect(),
                    return_type: Box::new(ext_fn.return_ty.clone()),
                },
                ext_fn.visibility,
                true,
            );
        }
        Ok(())
    }

    fn collect_structs(&mut self, structs: &[crate::ast::StructDef]) -> AnalysisResult<()> {
        for s in structs {
            if self.symbol_table.resolve(&s.name).is_some() {
                return Err(AnalysisError::new_with_span(
                    &format!("Duplicate declaration of type '{}'", s.name),
                    &s.span,
                )
                .with_module("global"));
            }
            self.symbol_table.define_with_generics(
                s.name.clone(),
                Type::Custom {
                    name: s.name.clone(),
                    generic_args: vec![],
                    is_exported: s.visibility.is_public(),
                },
                s.visibility,
                true,
                s.generic_params.clone(),
                None,
            );

            // Also register struct methods in the symbol table
            // Methods are named as StructName_methodname for external access
            for method in &s.methods {
                let method_name = format!("{}_{}", s.name, method.name);
                if self.symbol_table.resolve(&method_name).is_some() {
                    return Err(AnalysisError::new_with_span(
                        &format!("Duplicate declaration of method '{}'", method_name),
                        &method.span,
                    )
                    .with_module("global"));
                }
                let mut all_params = s.generic_params.clone();
                all_params.extend(method.generic_params.clone());
                self.symbol_table.define_with_generics(
                    method_name,
                    Type::Function {
                        params: method.params.iter().map(|p| p.ty.clone()).collect(),
                        return_type: Box::new(method.return_ty.clone()),
                    },
                    method.visibility,
                    true,
                    all_params,
                    None,
                );
            }
        }
        Ok(())
    }

    fn collect_interfaces(
        &mut self,
        interfaces: &[crate::ast::InterfaceDef],
    ) -> AnalysisResult<()> {
        for interface in interfaces {
            if self.symbol_table.resolve(&interface.name).is_some() {
                return Err(AnalysisError::new_with_span(
                    &format!("Duplicate declaration of interface '{}'", interface.name),
                    &interface.span,
                )
                .with_module("global"));
            }
            self.symbol_table.define(
                interface.name.clone(),
                Type::Custom {
                    name: interface.name.clone(),
                    generic_args: vec![],
                    is_exported: interface.visibility.is_public(),
                },
                interface.visibility,
                true,
            );
        }
        Ok(())
    }

    fn collect_enums(&mut self, enums: &[crate::ast::EnumDef]) -> AnalysisResult<()> {
        for e in enums {
            if self.symbol_table.resolve(&e.name).is_some() {
                return Err(AnalysisError::new_with_span(
                    &format!("Duplicate declaration of type '{}'", e.name),
                    &e.span,
                )
                .with_module("global"));
            }
            self.symbol_table.define_with_generics(
                e.name.clone(),
                Type::Custom {
                    name: e.name.clone(),
                    generic_args: vec![],
                    is_exported: e.visibility.is_public(),
                },
                e.visibility,
                true,
                e.generic_params.clone(),
                None,
            );

            // Also register enum variants in the symbol table
            for variant in &e.variants {
                // Register variant with fully qualified name: EnumName.VariantName
                let variant_name = format!("{}.{}", e.name, variant.name);
                self.symbol_table.define(
                    variant_name,
                    Type::Custom {
                        name: e.name.clone(),
                        generic_args: vec![],
                        is_exported: e.visibility.is_public(),
                    },
                    variant.visibility,
                    true,
                );
            }

            // Also register enum methods in the symbol table
            for method in &e.methods {
                let method_name = format!("{}_{}", e.name, method.name);
                if self.symbol_table.resolve(&method_name).is_some() {
                    return Err(AnalysisError::new_with_span(
                        &format!("Duplicate declaration of method '{}'", method_name),
                        &method.span,
                    )
                    .with_module("global"));
                }
                let mut all_params = e.generic_params.clone();
                all_params.extend(method.generic_params.clone());
                self.symbol_table.define_with_generics(
                    method_name,
                    Type::Function {
                        params: method.params.iter().map(|p| p.ty.clone()).collect(),
                        return_type: Box::new(method.return_ty.clone()),
                    },
                    method.visibility,
                    true,
                    all_params,
                    None,
                );
            }
        }
        Ok(())
    }

    fn collect_errors(&mut self, errors: &[crate::ast::ErrorDef]) -> AnalysisResult<()> {
        for e in errors {
            if self.symbol_table.resolve(&e.name).is_some() {
                return Err(AnalysisError::new_with_span(
                    &format!("Duplicate declaration of error type '{}'", e.name),
                    &e.span,
                )
                .with_module("global"));
            }
            self.symbol_table
                .define(e.name.clone(), Type::Error, e.visibility, true);

            // Also register error variants in the symbol table
            for variant in &e.variants {
                // Register variant with fully qualified name: ErrorType.VariantName
                let variant_name = format!("{}.{}", e.name, variant.name);
                self.symbol_table
                    .define(variant_name, Type::Error, variant.visibility, true);
            }
        }
        Ok(())
    }

    fn validate_interface_impls(&self, program: &crate::ast::Program) -> AnalysisResult<()> {
        let interfaces: HashMap<String, &crate::ast::InterfaceDef> = program
            .interfaces
            .iter()
            .map(|interface| (interface.name.clone(), interface))
            .collect();

        for strukt in &program.structs {
            for interface_impl in &strukt.interface_impls {
                let interface =
                    interfaces
                        .get(&interface_impl.interface_name)
                        .ok_or_else(|| {
                            AnalysisError::new_with_span(
                                &format!(
                                    "Unknown interface '{}' in impl block for struct '{}'",
                                    interface_impl.interface_name, strukt.name
                                ),
                                &interface_impl.span,
                            )
                            .with_module("global")
                        })?;

                let mut missing_methods = Vec::new();
                for required_method in &interface.methods {
                    let impl_method = interface_impl
                        .methods
                        .iter()
                        .find(|method| method.name == required_method.name);

                    match impl_method {
                        Some(method) => {
                            if !interface_method_compatible(required_method, method) {
                                return Err(AnalysisError::new_with_span(
                                    &format!(
                                        "Method '{}.{}' does not match interface signature",
                                        strukt.name, required_method.name
                                    ),
                                    &method.span,
                                )
                                .with_module("global"));
                            }
                        }
                        None => missing_methods.push(required_method.name.clone()),
                    }
                }

                if !missing_methods.is_empty() {
                    return Err(AnalysisError::new_with_span(
                        &format!(
                            "Struct '{}' does not fully implement interface '{}'. Missing methods: {}",
                            strukt.name,
                            interface.name,
                            missing_methods.join(", ")
                        ),
                        &interface_impl.span,
                    )
                    .with_module("global"));
                }
            }
        }

        Ok(())
    }

    pub fn get_symbol_table(&self) -> &SymbolTable {
        &self.symbol_table
    }
}

fn interface_method_compatible(
    interface_method: &crate::ast::FnDef,
    impl_method: &crate::ast::FnDef,
) -> bool {
    if interface_method.return_ty != impl_method.return_ty {
        return false;
    }

    let impl_params = if impl_method
        .params
        .first()
        .is_some_and(|param| is_self_receiver_type(&param.ty))
    {
        &impl_method.params[1..]
    } else {
        impl_method.params.as_slice()
    };

    if interface_method.params.len() != impl_params.len() {
        return false;
    }

    interface_method
        .params
        .iter()
        .zip(impl_params.iter())
        .all(|(required, actual)| required.ty == actual.ty)
}

fn is_self_receiver_type(ty: &Type) -> bool {
    match ty {
        Type::SelfType => true,
        Type::Custom { name, .. } => name == "Self",
        Type::Pointer(inner) => is_self_receiver_type(inner),
        _ => false,
    }
}
