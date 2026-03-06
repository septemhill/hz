use crate::ast::{Span, Visibility};
use crate::sema::error::{AnalysisError, AnalysisResult};
use crate::sema::symbol::SymbolTable;

// ============================================================================
// Analysis Pass 3: Symbol Resolver
// Resolves variable/function references and checks scope correctness
// ============================================================================

pub struct SymbolResolver {
    symbol_table: SymbolTable,
}

impl SymbolResolver {
    pub fn new(symbol_table: SymbolTable) -> Self {
        SymbolResolver { symbol_table }
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
            crate::ast::Stmt::Expr { expr, .. } => {
                self.analyze_expression(expr)?;
                Ok(())
            }
            crate::ast::Stmt::Import { .. } => Ok(()),
            crate::ast::Stmt::Let {
                name,
                names,
                ty,
                value,
                ..
            } => {
                if let Some(val_expr) = value {
                    self.analyze_expression(val_expr)?;
                }
                let inferred_ty = value
                    .as_ref()
                    .map(|_| crate::ast::Type::I64)
                    .unwrap_or_else(|| ty.clone().unwrap_or(crate::ast::Type::I64));
                if let Some(ns) = names {
                    for name_opt in ns {
                        if let Some(n) = name_opt {
                            self.symbol_table.define(
                                n.clone(),
                                inferred_ty.clone(),
                                Visibility::Private,
                                false,
                            );
                        }
                    }
                } else {
                    self.symbol_table
                        .define(name.clone(), inferred_ty, Visibility::Private, false);
                }
                Ok(())
            }
            crate::ast::Stmt::Assign {
                target,
                value,
                op: _,
                span,
            } => {
                if target != "_" {
                    self.symbol_table.resolve(target).ok_or_else(|| {
                        AnalysisError::new_with_span(
                            &format!("Undefined variable '{}'", target),
                            span,
                        )
                    })?;
                }
                self.analyze_expression(value)?;
                Ok(())
            }
            crate::ast::Stmt::Return { value, .. } => {
                if let Some(val_expr) = value {
                    self.analyze_expression(val_expr)?;
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
                condition,
                then_branch,
                else_branch,
                capture,
                ..
            } => {
                self.analyze_expression(condition)?;
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
            crate::ast::Stmt::While {
                condition,
                body,
                capture,
                ..
            } => {
                self.analyze_expression(condition)?;
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
                iterable,
                capture,
                index_var,
                body,
                ..
            } => {
                self.analyze_expression(iterable)?;
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
            crate::ast::Stmt::Switch {
                condition, cases, ..
            } => {
                self.analyze_expression(condition)?;
                for case in cases {
                    self.analyze_statement(&case.body)?;
                }
                Ok(())
            }
            crate::ast::Stmt::Defer { stmt, .. } => {
                self.analyze_statement(stmt)?;
                Ok(())
            }
        }
    }

    fn analyze_expression(&mut self, expr: &crate::ast::Expr) -> AnalysisResult<crate::ast::Type> {
        match expr {
            crate::ast::Expr::Ident(name, span) => {
                if name == "_" {
                    return Ok(crate::ast::Type::I64);
                }
                self.symbol_table
                    .resolve(name)
                    .map(|s| s.ty.clone())
                    .ok_or_else(|| {
                        AnalysisError::new_with_span(
                            &format!("Undefined identifier '{}'", name),
                            span,
                        )
                    })
            }
            crate::ast::Expr::Call {
                name,
                namespace,
                args,
                span,
            } => {
                if namespace.as_deref() != Some("io") || name != "println" {
                    if namespace.is_none() {
                        self.symbol_table.resolve(name).ok_or_else(|| {
                            AnalysisError::new_with_span(
                                &format!("Undefined function '{}'", name),
                                span,
                            )
                        })?;
                    }
                }
                for arg in args {
                    self.analyze_expression(arg)?;
                }
                Ok(crate::ast::Type::I64)
            }
            crate::ast::Expr::Catch {
                expr,
                error_var,
                body,
                span: _,
            } => {
                self.analyze_expression(expr)?;
                let ev = error_var.clone();
                if ev.is_some() {
                    self.symbol_table.enter_scope();
                    self.symbol_table.define(
                        ev.unwrap(),
                        crate::ast::Type::Error,
                        Visibility::Private,
                        false,
                    );
                }
                self.analyze_expression(body)?;
                if error_var.is_some() {
                    self.symbol_table.exit_scope();
                }
                Ok(crate::ast::Type::I64)
            }
            crate::ast::Expr::Struct { name, fields, .. } => {
                self.symbol_table
                    .resolve(name)
                    .ok_or_else(|| AnalysisError::new(&format!("Undefined struct '{}'", name)))?;
                for (_, field_expr) in fields {
                    self.analyze_expression(field_expr)?;
                }
                Ok(crate::ast::Type::I64)
            }
            crate::ast::Expr::Binary { left, right, .. } => {
                self.analyze_expression(left)?;
                self.analyze_expression(right)?;
                Ok(crate::ast::Type::I64)
            }
            crate::ast::Expr::Unary { expr, .. } => {
                self.analyze_expression(expr)?;
                Ok(crate::ast::Type::I64)
            }
            crate::ast::Expr::MemberAccess { object, .. } => {
                self.analyze_expression(object)?;
                Ok(crate::ast::Type::I64)
            }
            crate::ast::Expr::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.analyze_expression(condition)?;
                self.analyze_expression(then_branch)?;
                self.analyze_expression(else_branch)?;
                Ok(crate::ast::Type::I64)
            }
            crate::ast::Expr::Block { stmts, .. } => {
                self.symbol_table.enter_scope();
                for s in stmts {
                    self.analyze_statement(s)?;
                }
                self.symbol_table.exit_scope();
                Ok(crate::ast::Type::I64)
            }
            crate::ast::Expr::Try { expr, .. } => {
                self.analyze_expression(expr)?;
                Ok(crate::ast::Type::I64)
            }
            _ => Ok(crate::ast::Type::I64),
        }
    }

    pub fn get_symbol_table(&self) -> &SymbolTable {
        &self.symbol_table
    }
}
