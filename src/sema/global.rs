use crate::ast::{Type, Visibility};
use crate::sema::error::{AnalysisError, AnalysisResult};
use crate::sema::symbol::SymbolTable;

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
        self.collect_structs(&program.structs)?;
        self.collect_enums(&program.enums)?;
        self.collect_errors(&program.errors)?;
        Ok(self.symbol_table.clone())
    }

    fn collect_functions(&mut self, functions: &[crate::ast::FnDef]) -> AnalysisResult<()> {
        for f in functions {
            if self.symbol_table.resolve(&f.name).is_some() {
                return Err(AnalysisError::new(&format!(
                    "Duplicate declaration of function '{}'",
                    f.name
                )));
            }
            self.symbol_table
                .define(f.name.clone(), f.return_ty.clone(), f.visibility, true);
        }
        Ok(())
    }

    fn collect_external_functions(
        &mut self,
        ext_fns: &[crate::ast::ExternalFnDef],
    ) -> AnalysisResult<()> {
        for ext_fn in ext_fns {
            if self.symbol_table.resolve(&ext_fn.name).is_some() {
                return Err(AnalysisError::new(&format!(
                    "Duplicate declaration of external function '{}'",
                    ext_fn.name
                )));
            }
            self.symbol_table.define(
                ext_fn.name.clone(),
                ext_fn.return_ty.clone(),
                ext_fn.visibility,
                true,
            );
        }
        Ok(())
    }

    fn collect_structs(&mut self, structs: &[crate::ast::StructDef]) -> AnalysisResult<()> {
        for s in structs {
            if self.symbol_table.resolve(&s.name).is_some() {
                return Err(AnalysisError::new(&format!(
                    "Duplicate declaration of type '{}'",
                    s.name
                )));
            }
            self.symbol_table.define(
                s.name.clone(),
                Type::Custom {
                    name: s.name.clone(),
                    generic_args: vec![],
                    is_exported: s.visibility.is_public(),
                },
                s.visibility,
                true,
            );
        }
        Ok(())
    }

    fn collect_enums(&mut self, enums: &[crate::ast::EnumDef]) -> AnalysisResult<()> {
        for e in enums {
            if self.symbol_table.resolve(&e.name).is_some() {
                return Err(AnalysisError::new(&format!(
                    "Duplicate declaration of type '{}'",
                    e.name
                )));
            }
            self.symbol_table.define(
                e.name.clone(),
                Type::Custom {
                    name: e.name.clone(),
                    generic_args: vec![],
                    is_exported: e.visibility.is_public(),
                },
                e.visibility,
                true,
            );
        }
        Ok(())
    }

    fn collect_errors(&mut self, errors: &[crate::ast::ErrorDef]) -> AnalysisResult<()> {
        for e in errors {
            if self.symbol_table.resolve(&e.name).is_some() {
                return Err(AnalysisError::new(&format!(
                    "Duplicate declaration of error type '{}'",
                    e.name
                )));
            }
            self.symbol_table
                .define(e.name.clone(), Type::Error, e.visibility, true);
        }
        Ok(())
    }

    pub fn get_symbol_table(&self) -> &SymbolTable {
        &self.symbol_table
    }
}
