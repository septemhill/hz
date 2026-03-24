use crate::ast::Visibility;
use crate::sema::error::{AnalysisError, AnalysisResult};
use crate::sema::infer::TypedProgram;
use crate::sema::symbol::SymbolTable;

// ============================================================================
// Analysis Pass 4: Mutability Analyzer
// Checks that const variables aren't reassigned
// ============================================================================

pub struct MutabilityAnalyzer {
    symbol_table: SymbolTable,
    #[allow(unused)]
    typed_program: TypedProgram,
}

impl MutabilityAnalyzer {
    pub fn new(symbol_table: SymbolTable, typed_program: TypedProgram) -> Self {
        MutabilityAnalyzer {
            symbol_table,
            typed_program,
        }
    }

    pub fn analyze(&mut self, program: &crate::ast::Program) -> AnalysisResult<()> {
        for f in &program.functions {
            if f.generic_params.is_empty() {
                self.analyze_function(f)?;
            }
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
                    // Check if this is a member access (contains a dot)
                    if target.contains('.') {
                        // For member access like "self.i", we need to handle it specially
                        // Split by dot - the first part is the base identifier
                        let parts: Vec<&str> = target.split('.').collect();
                        if let Some(base) = parts.first() {
                            // Check if the base identifier is in the symbol table
                            // or if it's a special case like 'self' (method receiver)
                            if *base != "self" {
                                let is_const = self
                                    .symbol_table
                                    .resolve(base)
                                    .map(|s| s.is_const)
                                    .ok_or_else(|| {
                                        AnalysisError::new_with_span(
                                            &format!("Undefined variable '{}'", base),
                                            span,
                                        )
                                        .with_module("mutability")
                                    })?;
                                if is_const {
                                    return Err(AnalysisError::new_with_span(
                                        &format!("Cannot reassign constant variable '{}'", base),
                                        span,
                                    )
                                    .with_module("mutability"));
                                }
                            }
                            // For member assignments, we skip const check for now
                            // The field mutability would need more complex handling
                        }
                    } else {
                        // Regular variable assignment
                        let is_const = self
                            .symbol_table
                            .resolve(target)
                            .map(|s| s.is_const)
                            .ok_or_else(|| {
                                AnalysisError::new_with_span(
                                    &format!("Undefined variable '{}'", target),
                                    span,
                                )
                                .with_module("mutability")
                            })?;
                        if is_const {
                            return Err(AnalysisError::new_with_span(
                                &format!("Cannot reassign constant variable '{}'", target),
                                span,
                            )
                            .with_module("mutability"));
                        }
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
            crate::ast::Stmt::Let {
                name,
                names,
                ty,
                value,
                mutability,
                visibility,
                span: _,
            } => {
                let is_const = *mutability == crate::ast::Mutability::Const;
                // Use I64 as default type when no type annotation and no value
                let default_ty = crate::ast::Type::I64;
                let inferred_ty = value
                    .as_ref()
                    .map(|_| default_ty.clone())
                    .unwrap_or_else(|| ty.clone().unwrap_or(default_ty));
                // Handle tuple destructuring
                if let Some(var_names) = names {
                    for var_name in var_names {
                        if let Some(n) = var_name {
                            self.symbol_table.define(
                                n.clone(),
                                inferred_ty.clone(),
                                *visibility,
                                is_const,
                            );
                        }
                    }
                } else {
                    self.symbol_table
                        .define(name.clone(), inferred_ty, *visibility, is_const);
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    pub fn get_symbol_table(&self) -> &SymbolTable {
        &self.symbol_table
    }
}
