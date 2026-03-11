use crate::ast::{Span, Visibility};
use crate::sema::error::{AnalysisError, AnalysisResult};
use crate::sema::symbol::SymbolTable;

// ============================================================================
// Analysis Pass 2: Type Analyzer
// Performs type checking for expressions and statements
// ============================================================================

pub struct TypeAnalyzer {
    symbol_table: SymbolTable,
}

impl TypeAnalyzer {
    pub fn new(symbol_table: SymbolTable) -> Self {
        TypeAnalyzer { symbol_table }
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

        // Check: if function body contains try expression, return type must be Result
        if let Some(try_span) = self.find_try_expression(&f.body) {
            if !f.return_ty.is_result() {
                return Err(AnalysisError::new_with_span(
                    &format!(
                        "Function '{}' contains try expression but does not return a Result type. Try expressions require the function to return a Result type to propagate errors.",
                        f.name
                    ),
                    &try_span,
                ));
            }
        }

        Ok(())
    }

    /// Find the span of any try expression in the statement list, if one exists
    fn find_try_expression(&self, stmts: &[crate::ast::Stmt]) -> Option<Span> {
        for stmt in stmts {
            if let Some(span) = self.stmt_find_try(stmt) {
                return Some(span);
            }
        }
        None
    }

    /// Find the span of a try expression in a statement (recursively)
    fn stmt_find_try(&self, stmt: &crate::ast::Stmt) -> Option<Span> {
        match stmt {
            crate::ast::Stmt::Expr { expr, .. } => {
                // Return the span of the actual try expression, not the Expr statement
                self.expr_find_try(expr)
            }
            crate::ast::Stmt::Let { value, .. } => {
                if let Some(val) = value {
                    self.expr_find_try(val)
                } else {
                    None
                }
            }
            crate::ast::Stmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                if let Some(s) = self.stmt_find_try(then_branch) {
                    return Some(s);
                }
                if let Some(eb) = else_branch {
                    return self.stmt_find_try(eb);
                }
                None
            }
            crate::ast::Stmt::While { body, .. } => self.stmt_find_try(body),
            crate::ast::Stmt::For { body, .. } => self.stmt_find_try(body),
            crate::ast::Stmt::Loop { body, .. } => self.stmt_find_try(body),
            crate::ast::Stmt::Switch { cases, .. } => {
                for case in cases {
                    if let Some(s) = self.stmt_find_try(&case.body) {
                        return Some(s);
                    }
                }
                None
            }
            crate::ast::Stmt::Block { stmts, .. } => self.find_try_expression(stmts),
            crate::ast::Stmt::Defer { stmt, .. } => self.stmt_find_try(stmt),
            crate::ast::Stmt::DeferBang { stmt, .. } => self.stmt_find_try(stmt),
            crate::ast::Stmt::Return { value, .. } => {
                if let Some(val) = value {
                    self.expr_find_try(val)
                } else {
                    None
                }
            }
            crate::ast::Stmt::Assign { value, .. } => self.expr_find_try(value),
            crate::ast::Stmt::Import { .. } => None,
        }
    }

    /// Find the span of a try expression in an expression, if it exists
    fn expr_find_try(&self, expr: &crate::ast::Expr) -> Option<Span> {
        match expr {
            crate::ast::Expr::Try { span, .. } => Some(*span),
            crate::ast::Expr::Call { args, .. } => {
                for arg in args {
                    if let Some(s) = self.expr_find_try(arg) {
                        return Some(s);
                    }
                }
                None
            }
            crate::ast::Expr::Binary { left, right, .. } => {
                if let Some(s) = self.expr_find_try(left) {
                    return Some(s);
                }
                self.expr_find_try(right)
            }
            crate::ast::Expr::Unary { expr, .. } => self.expr_find_try(expr),
            crate::ast::Expr::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                if let Some(s) = self.expr_find_try(condition) {
                    return Some(s);
                }
                if let Some(s) = self.expr_find_try(then_branch) {
                    return Some(s);
                }
                self.expr_find_try(else_branch)
            }
            crate::ast::Expr::Block { stmts, .. } => self.find_try_expression(stmts),
            crate::ast::Expr::MemberAccess { object, .. } => self.expr_find_try(object),
            crate::ast::Expr::Catch { expr, body, .. } => {
                if let Some(s) = self.expr_find_try(expr) {
                    return Some(s);
                }
                self.expr_find_try(body)
            }
            crate::ast::Expr::Struct { fields, .. } => {
                for (_, field_expr) in fields {
                    if let Some(s) = self.expr_find_try(field_expr) {
                        return Some(s);
                    }
                }
                None
            }
            crate::ast::Expr::Array(elements, _) | crate::ast::Expr::Tuple(elements, _) => {
                for elem in elements {
                    if let Some(s) = self.expr_find_try(elem) {
                        return Some(s);
                    }
                }
                None
            }
            crate::ast::Expr::TupleIndex { tuple, .. } => self.expr_find_try(tuple),
            // Base cases - no try expression
            crate::ast::Expr::Int(_, _)
            | crate::ast::Expr::Bool(_, _)
            | crate::ast::Expr::String(_, _)
            | crate::ast::Expr::Char(_, _)
            | crate::ast::Expr::Null(_)
            | crate::ast::Expr::Ident(_, _) => None,
        }
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
                mutability,
                visibility: _,
                span,
            } => {
                let inferred_ty = if let Some(val_expr) = value {
                    let v_ty = self.analyze_expression(val_expr)?;
                    if let Some(explicit_ty) = ty {
                        if !self.types_compatible(explicit_ty, &v_ty) {
                            return Err(AnalysisError::new_with_span(
                                &format!(
                                    "Type mismatch in variable declaration: expected {}, found {}",
                                    explicit_ty, v_ty
                                ),
                                span,
                            ));
                        }
                    }
                    v_ty
                } else if let Some(explicit_ty) = ty {
                    explicit_ty.clone()
                } else {
                    return Err(AnalysisError::new_with_span(
                        "Variable must have either a type or an initial value",
                        span,
                    ));
                };

                // Handle both single name and tuple destructuring
                // Check for duplicate variable declarations in the same scope
                if let Some(ns) = names {
                    for name_opt in ns {
                        if let Some(n) = name_opt {
                            if self.symbol_table.contains(n) {
                                return Err(AnalysisError::new_with_span(
                                    &format!("Variable '{}' is already declared in this scope", n),
                                    span,
                                ));
                            }
                            self.symbol_table.define(
                                n.clone(),
                                inferred_ty.clone(),
                                Visibility::Private,
                                matches!(mutability, crate::ast::Mutability::Const),
                            );
                        }
                    }
                } else {
                    if self.symbol_table.contains(name) {
                        return Err(AnalysisError::new_with_span(
                            &format!("Variable '{}' is already declared in this scope", name),
                            span,
                        ));
                    }
                    self.symbol_table.define(
                        name.clone(),
                        inferred_ty,
                        Visibility::Private,
                        matches!(mutability, crate::ast::Mutability::Const),
                    );
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
                    let symbol_ty = self
                        .symbol_table
                        .resolve(target)
                        .map(|s| s.ty.clone())
                        .ok_or_else(|| {
                            AnalysisError::new_with_span(
                                &format!("Undefined variable '{}'", target),
                                span,
                            )
                        })?;
                    let expr_ty = self.analyze_expression(value)?;
                    if !self.types_compatible(&symbol_ty, &expr_ty) {
                        return Err(AnalysisError::new_with_span(
                            &format!(
                                "Type mismatch in assignment to '{}': expected {}, found {}",
                                target, symbol_ty, expr_ty
                            ),
                            span,
                        ));
                    }
                }
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
                capture,
                then_branch,
                else_branch,
                span,
            } => {
                let cond_ty = self.analyze_expression(condition)?;

                // Handle capture variable if present
                if let Some(cap) = capture {
                    // If condition is an optional type, the capture gets the inner type
                    if let crate::ast::Type::Option(inner_ty) = cond_ty {
                        self.symbol_table.define(
                            cap.clone(),
                            *inner_ty,
                            Visibility::Private,
                            false,
                        );
                    } else {
                        return Err(AnalysisError::new_with_span(
                            "Capture variable requires an optional type",
                            span,
                        ));
                    }
                }

                self.analyze_statement(then_branch)?;
                if let Some(eb) = else_branch {
                    self.analyze_statement(eb)?;
                }
                Ok(())
            }
            crate::ast::Stmt::While {
                condition, body, ..
            } => {
                self.analyze_expression(condition)?;
                self.analyze_statement(body)?;
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
            crate::ast::Stmt::Defer {
                stmt: deferred_stmt,
                ..
            } => {
                self.analyze_statement(deferred_stmt)?;
                Ok(())
            }
            crate::ast::Stmt::DeferBang {
                stmt: deferred_stmt,
                ..
            } => {
                // DeferBang is similar to Defer but only executes on error
                self.analyze_statement(deferred_stmt)?;
                Ok(())
            }
        }
    }

    fn analyze_expression(&mut self, expr: &crate::ast::Expr) -> AnalysisResult<crate::ast::Type> {
        match expr {
            crate::ast::Expr::Int(_, _) => Ok(crate::ast::Type::I64),
            crate::ast::Expr::Bool(_, _) => Ok(crate::ast::Type::Bool),
            crate::ast::Expr::String(_, _) => Ok(crate::ast::Type::Custom {
                name: "String".to_string(),
                generic_args: vec![],
                is_exported: false,
            }),
            crate::ast::Expr::Char(_, _) => Ok(crate::ast::Type::I8),
            crate::ast::Expr::Null(_) => {
                Ok(crate::ast::Type::Option(Box::new(crate::ast::Type::I64)))
            }
            crate::ast::Expr::Tuple(elements, _) => {
                let mut types = vec![];
                for elem in elements {
                    let ty = self.analyze_expression(elem)?;
                    types.push(ty);
                }
                Ok(crate::ast::Type::Tuple(types))
            }
            crate::ast::Expr::TupleIndex { tuple, index, span } => {
                let tuple_ty = self.analyze_expression(tuple)?;
                if let crate::ast::Type::Tuple(types) = tuple_ty {
                    if *index < types.len() {
                        Ok(types[*index].clone())
                    } else {
                        Err(AnalysisError::new_with_span(
                            "Tuple index out of bounds",
                            span,
                        ))
                    }
                } else {
                    Err(AnalysisError::new_with_span(
                        "Tuple index on non-tuple type",
                        span,
                    ))
                }
            }
            crate::ast::Expr::Ident(name, span) => {
                if name == "_" {
                    return Ok(crate::ast::Type::I64);
                }
                self.symbol_table
                    .resolve(name)
                    .map(|s| s.ty.clone())
                    .ok_or_else(|| {
                        AnalysisError::new_with_span(
                            &format!("Undefined variable '{}'", name),
                            span,
                        )
                    })
            }
            crate::ast::Expr::Array(elements, _) => {
                for elem in elements {
                    self.analyze_expression(elem)?;
                }
                Ok(crate::ast::Type::I64)
            }
            crate::ast::Expr::Binary {
                op,
                left,
                right,
                span,
            } => {
                let l_ty = self.analyze_expression(left)?;
                let r_ty = self.analyze_expression(right)?;
                match op {
                    crate::ast::BinaryOp::Add
                    | crate::ast::BinaryOp::Sub
                    | crate::ast::BinaryOp::Mul
                    | crate::ast::BinaryOp::Div
                    | crate::ast::BinaryOp::Mod
                    | crate::ast::BinaryOp::BitAnd
                    | crate::ast::BinaryOp::BitOr
                    | crate::ast::BinaryOp::BitXor
                    | crate::ast::BinaryOp::Shl
                    | crate::ast::BinaryOp::Shr => {
                        if self.is_numeric(&l_ty) && self.is_numeric(&r_ty) {
                            Ok(l_ty)
                        } else {
                            Err(AnalysisError::new_with_span(
                                "Binary operation requires numeric operands",
                                span,
                            ))
                        }
                    }
                    crate::ast::BinaryOp::Eq
                    | crate::ast::BinaryOp::Ne
                    | crate::ast::BinaryOp::Lt
                    | crate::ast::BinaryOp::Gt
                    | crate::ast::BinaryOp::Le
                    | crate::ast::BinaryOp::Ge => {
                        if self.types_compatible(&l_ty, &r_ty) {
                            Ok(crate::ast::Type::Bool)
                        } else {
                            Err(AnalysisError::new_with_span(
                                "Comparison requires compatible types",
                                span,
                            ))
                        }
                    }
                    crate::ast::BinaryOp::And | crate::ast::BinaryOp::Or => {
                        if l_ty == crate::ast::Type::Bool && r_ty == crate::ast::Type::Bool {
                            Ok(crate::ast::Type::Bool)
                        } else {
                            Err(AnalysisError::new_with_span(
                                "Logical operation requires boolean operands",
                                span,
                            ))
                        }
                    }
                    crate::ast::BinaryOp::Range => Ok(crate::ast::Type::I64),
                }
            }
            crate::ast::Expr::Unary { op, expr, span } => {
                let e_ty = self.analyze_expression(expr)?;
                match op {
                    crate::ast::UnaryOp::Neg | crate::ast::UnaryOp::Pos => {
                        if self.is_numeric(&e_ty) {
                            Ok(e_ty)
                        } else {
                            Err(AnalysisError::new_with_span(
                                "Unary requires numeric operand",
                                span,
                            ))
                        }
                    }
                    crate::ast::UnaryOp::Not => {
                        if e_ty == crate::ast::Type::Bool {
                            Ok(crate::ast::Type::Bool)
                        } else {
                            Err(AnalysisError::new_with_span(
                                "Logical NOT requires boolean operand",
                                span,
                            ))
                        }
                    }
                }
            }
            crate::ast::Expr::Call {
                name,
                namespace,
                args,
                span,
            } => {
                if namespace.as_deref() == Some("io") && name == "println" {
                    for arg in args {
                        self.analyze_expression(arg)?;
                    }
                    return Ok(crate::ast::Type::Void);
                }
                let symbol_ty = if namespace.is_some() {
                    crate::ast::Type::I64
                } else {
                    self.symbol_table
                        .resolve(name)
                        .map(|s| s.ty.clone())
                        .ok_or_else(|| {
                            AnalysisError::new_with_span(
                                &format!("Undefined function '{}'", name),
                                span,
                            )
                        })?
                };
                for arg in args {
                    self.analyze_expression(arg)?;
                }
                Ok(symbol_ty)
            }
            crate::ast::Expr::If {
                condition,
                then_branch,
                else_branch,
                capture: _,
                span,
            } => {
                self.analyze_expression(condition)?;
                let then_ty = self.analyze_expression(then_branch)?;
                let else_ty = self.analyze_expression(else_branch)?;
                if self.types_compatible(&then_ty, &else_ty) {
                    Ok(then_ty)
                } else {
                    Err(AnalysisError::new_with_span(
                        "If expression branches must have compatible types",
                        span,
                    ))
                }
            }
            crate::ast::Expr::Block { stmts, .. } => {
                self.symbol_table.enter_scope();
                for s in stmts {
                    self.analyze_statement(s)?;
                }
                self.symbol_table.exit_scope();
                Ok(crate::ast::Type::I64)
            }
            crate::ast::Expr::MemberAccess { object, .. } => {
                self.analyze_expression(object)?;
                Ok(crate::ast::Type::I64)
            }
            crate::ast::Expr::Struct { name, fields, span } => {
                self.symbol_table
                    .resolve(name)
                    .map(|s| s.ty.clone())
                    .ok_or_else(|| {
                        AnalysisError::new_with_span(&format!("Undefined struct '{}'", name), span)
                    })?;
                for (_, field_expr) in fields {
                    self.analyze_expression(field_expr)?;
                }
                Ok(crate::ast::Type::Custom {
                    name: name.clone(),
                    generic_args: vec![],
                    is_exported: false,
                })
            }
            crate::ast::Expr::Try { expr, span } => {
                let expr_ty = self.analyze_expression(expr)?;
                if expr_ty.is_result() {
                    expr_ty.result_inner().cloned().ok_or_else(|| {
                        AnalysisError::new_with_span("Try expression requires Result type", span)
                    })
                } else {
                    Ok(expr_ty)
                }
            }
            crate::ast::Expr::Catch {
                expr,
                error_var,
                body,
                span,
            } => {
                let expr_ty = self.analyze_expression(expr)?;
                if !expr_ty.is_result() {
                    return Err(AnalysisError::new(&format!(
                        "catch expression requires a Result type, expected Result<T> but found {}",
                        expr_ty
                    )));
                }
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
                expr_ty
                    .result_inner()
                    .cloned()
                    .ok_or_else(|| AnalysisError::new("Failed to get inner type from Result"))
            }
        }
    }

    fn is_numeric(&self, ty: &crate::ast::Type) -> bool {
        matches!(
            ty,
            crate::ast::Type::I8
                | crate::ast::Type::I16
                | crate::ast::Type::I32
                | crate::ast::Type::I64
                | crate::ast::Type::U8
                | crate::ast::Type::U16
                | crate::ast::Type::U32
                | crate::ast::Type::U64
        )
    }

    fn types_compatible(&self, left: &crate::ast::Type, right: &crate::ast::Type) -> bool {
        // Check if left is an Option and right is the inner type
        if let crate::ast::Type::Option(inner) = left {
            if **inner == *right {
                return true;
            }
        }
        // Check if right is an Option and left is the inner type
        if let crate::ast::Type::Option(inner) = right {
            if **inner == *left {
                return true;
            }
        }
        left == right || (self.is_numeric(left) && self.is_numeric(right))
    }

    pub fn get_symbol_table(&self) -> &SymbolTable {
        &self.symbol_table
    }
}
