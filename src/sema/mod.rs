// Semantic analysis module
// Provides symbol table management and various analysis passes

pub mod error;
pub mod global;
pub mod infer;
pub mod mutability;
pub mod resolver;
pub mod symbol;
pub mod types;

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
}

impl SemanticAnalyzer {
    pub fn new() -> Self {
        SemanticAnalyzer {
            symbol_table: SymbolTable::new(),
            typed_program: None,
        }
    }

    pub fn analyze(&mut self, program: &crate::ast::Program) -> AnalysisResult<()> {
        // Pass 1: Collect and validate global definitions
        let mut global_analyzer = GlobalDefinitionsAnalyzer::new();
        global_analyzer.analyze(program)?;

        // Pass 2: Type inference - produce type-annotated AST (must run early for function types)
        let symbol_table = global_analyzer.get_symbol_table().clone();
        let typed_prog = infer_types(program, symbol_table.clone())?;
        self.typed_program = Some(typed_prog.clone());

        // Pass 3: Type analysis - use the SAME symbol table that was passed to infer_types
        let mut type_analyzer = TypeAnalyzer::new(symbol_table);
        type_analyzer.analyze(program)?;

        // Pass 4: Symbol resolution
        let symbol_table = type_analyzer.get_symbol_table().clone();
        let mut symbol_resolver =
            SymbolResolver::new(symbol_table, program.structs.clone(), program.enums.clone());
        symbol_resolver.analyze(program)?;

        // Pass 5: Mutability analysis
        let symbol_table = symbol_resolver.get_symbol_table().clone();
        let mut mutability_analyzer = MutabilityAnalyzer::new(symbol_table, typed_prog);
        mutability_analyzer.analyze(program)?;

        // Store final symbol table
        self.symbol_table = mutability_analyzer.get_symbol_table().clone();

        Ok(())
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
