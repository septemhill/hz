use crate::ast::{Span, Visibility};
use crate::sema::error::{AnalysisError, AnalysisResult};
use crate::sema::symbol::SymbolTable;

// ============================================================================
// Analysis Pass 4: Mutability Analyzer
// Checks that const variables aren't reassigned
// ============================================================================

pub struct MutabilityAnalyzer {
    symbol_table: SymbolTable,
}

impl MutabilityAnalyzer {
    pub fn new(symbol_table: SymbolTable) -> Self {
        MutabilityAnalyzer { symbol_table }
    }

    pub fn analyze(&mut self, program: &crate::ast::Program) -> AnalysisResult<()> {
        for f in &program.functions {
            self.analyze_function(f)?;
        }
        Ok(())
    }

    fn analyze_function(&mut self, f: &crate::ast::FnDef) -> AnalysisResult<()> {
        self.symbol_table.enter_scope();
        for param in &f.params {
            self.symbol_table.define(
                param.name.clone(),
                param.ty.clone(),
                Visibility::Private,
                false,
            );
        }
        for stmt in &f.body {
            self.analyze_statement(stmt)?;
        }
        self.symbol_table.exit_scope();
        Ok(())
    }

    fn analyze_statement(&mut self, stmt: &crate::ast::Stmt) -> AnalysisResult<()> {
        match stmt {
            crate::ast::Stmt::Assign {
                target,
                value: _,
                op: _,
                span,
            } => {
                if target != "_" {
                    let is_const = self
                        .symbol_table
                        .resolve(target)
                        .map(|s| s.is_const)
                        .ok_or_else(|| {
                            AnalysisError::new_with_span(
                                &format!("Undefined variable '{}'", target),
                                span,
                            )
                        })?;
                    if is_const {
                        return Err(AnalysisError::new_with_span(
                            &format!("Cannot reassign constant variable '{}'", target),
                            span,
                        ));
                    }
                }
                Ok(())
            }
            crate::ast::Stmt::Block { stmts, .. } => {
                self.symbol_table.enter_scope();
                for s in stmts {
                    self.analyze_statement(s)?;
                }
                self.symbol_table.exit_scope();
                Ok(())
            }
            crate::ast::Stmt::If {
                then_branch,
                else_branch,
                capture,
                ..
            } => {
                let cap = capture.clone();
                if cap.is_some() {
                    self.symbol_table.enter_scope();
                    self.symbol_table.define(
                        cap.unwrap(),
                        crate::ast::Type::I64,
                        Visibility::Private,
                        false,
                    );
                    self.analyze_statement(then_branch)?;
                    self.symbol_table.exit_scope();
                } else {
                    self.analyze_statement(then_branch)?;
                }
                if let Some(eb) = else_branch {
                    self.analyze_statement(eb)?;
                }
                Ok(())
            }
            crate::ast::Stmt::While { body, capture, .. } => {
                let cap = capture.clone();
                if cap.is_some() {
                    self.symbol_table.enter_scope();
                    self.symbol_table.define(
                        cap.unwrap(),
                        crate::ast::Type::I64,
                        Visibility::Private,
                        false,
                    );
                    self.analyze_statement(body)?;
                    self.symbol_table.exit_scope();
                } else {
                    self.analyze_statement(body)?;
                }
                Ok(())
            }
            crate::ast::Stmt::For {
                var_name,
                iterable: _,
                capture,
                index_var,
                body,
                ..
            } => {
                self.symbol_table.enter_scope();
                if let Some(vn) = var_name {
                    self.symbol_table.define(
                        vn.clone(),
                        crate::ast::Type::I64,
                        Visibility::Private,
                        false,
                    );
                }
                if let Some(cv) = capture {
                    self.symbol_table.define(
                        cv.clone(),
                        crate::ast::Type::I64,
                        Visibility::Private,
                        false,
                    );
                }
                if let Some(iv) = index_var {
                    self.symbol_table.define(
                        iv.clone(),
                        crate::ast::Type::I64,
                        Visibility::Private,
                        false,
                    );
                }
                self.analyze_statement(body)?;
                self.symbol_table.exit_scope();
                Ok(())
            }
            crate::ast::Stmt::Loop { body, .. } => {
                self.analyze_statement(body)?;
                Ok(())
            }
            crate::ast::Stmt::Switch { cases, .. } => {
                for case in cases {
                    self.analyze_statement(&case.body)?;
                }
                Ok(())
            }
            crate::ast::Stmt::Defer { stmt, .. } => {
                self.analyze_statement(stmt)?;
                Ok(())
            }
            _ => Ok(()),
        }
    }

    pub fn get_symbol_table(&self) -> &SymbolTable {
        &self.symbol_table
    }
}
