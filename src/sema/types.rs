use crate::ast::{Span, Visibility};
use crate::sema::error::{AnalysisError, AnalysisResult};
use crate::sema::symbol::SymbolTable;
use std::collections::HashMap;

// ============================================================================
// Analysis Pass 2: Type Analyzer
// Performs type checking for expressions and statements
// ============================================================================

pub struct TypeAnalyzer {
    symbol_table: SymbolTable,
    structs: HashMap<String, crate::ast::StructDef>,
    enums: HashMap<String, crate::ast::EnumDef>,
    errors: HashMap<String, crate::ast::ErrorDef>,
}

fn destructured_binding_type(aggregate_ty: &crate::ast::Type, index: usize) -> crate::ast::Type {
    match aggregate_ty {
        crate::ast::Type::Tuple(types) => {
            types.get(index).cloned().unwrap_or(crate::ast::Type::I64)
        }
        _ => aggregate_ty.clone(),
    }
}

impl TypeAnalyzer {
    pub fn new(
        symbol_table: SymbolTable,
        structs: HashMap<String, crate::ast::StructDef>,
        enums: HashMap<String, crate::ast::EnumDef>,
        errors: HashMap<String, crate::ast::ErrorDef>,
    ) -> Self {
        TypeAnalyzer {
            symbol_table,
            structs,
            enums,
            errors,
        }
    }

    pub fn analyze(&mut self, program: &crate::ast::Program) -> AnalysisResult<()> {
        for f in &program.functions {
            if f.generic_params.is_empty() {
                self.analyze_function(f)?;
            }
        }
        // Also analyze struct methods
        for s in &program.structs {
            if s.generic_params.is_empty() {
                for method in &s.methods {
                    self.analyze_function(method)?;
                }
            }
        }
        // Also analyze enum methods
        for e in &program.enums {
            if e.generic_params.is_empty() {
                for method in &e.methods {
                    self.analyze_function(method)?;
                }
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

        // Check: if function body contains try expression, return type must be Result
        if let Some(try_span) = self.find_try_expression(&f.body) {
            if !f.return_ty.is_result() {
                return Err(AnalysisError::new_with_span(
                    &format!(
                        "Function '{}' contains try expression but does not return a Result type. Try expressions require the function to return a Result type to propagate errors.",
                        f.name
                    ),
                    &try_span,
                ).with_module("types"));
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
            crate::ast::Stmt::For { body, .. } => self.stmt_find_try(body),
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
            crate::ast::Stmt::Break { .. } => None,
            crate::ast::Stmt::Continue { .. } => None,
        }
    }

    fn validate_expr_against_type(
        &mut self,
        expr: &crate::ast::Expr,
        expected: &crate::ast::Type,
    ) -> AnalysisResult<()> {
        match expr {
            crate::ast::Expr::Int(value, span) if expected.is_integer() => {
                if expected.can_represent_int_literal(*value) {
                    Ok(())
                } else {
                    Err(AnalysisError::new_with_span(
                        &format!("Integer literal {} is out of range for {}", value, expected),
                        span,
                    )
                    .with_module("types"))
                }
            }
            crate::ast::Expr::Array(elements, explicit_ty, span) => {
                let expected_element_type = match expected {
                    crate::ast::Type::Array { element_type, .. } => {
                        Some(element_type.as_ref().clone())
                    }
                    _ => explicit_ty.clone(),
                };

                if let (
                    crate::ast::Type::Array {
                        size: Some(expected_size),
                        ..
                    },
                    elements_len,
                ) = (expected, elements.len())
                {
                    if *expected_size != elements_len {
                        return Err(AnalysisError::new_with_span(
                            &format!(
                                "Array literal expected {} elements, found {}",
                                expected_size, elements_len
                            ),
                            span,
                        )
                        .with_module("types"));
                    }
                }

                if let Some(element_type) = expected_element_type {
                    for elem in elements {
                        self.validate_expr_against_type(elem, &element_type)?;
                    }
                }

                Ok(())
            }
            crate::ast::Expr::Tuple(elements, span) => {
                if let crate::ast::Type::Tuple(expected_types) = expected {
                    if elements.len() != expected_types.len() {
                        return Err(AnalysisError::new_with_span(
                            &format!(
                                "Tuple literal expected {} elements, found {}",
                                expected_types.len(),
                                elements.len()
                            ),
                            span,
                        )
                        .with_module("types"));
                    }

                    for (elem, elem_ty) in elements.iter().zip(expected_types.iter()) {
                        self.validate_expr_against_type(elem, elem_ty)?;
                    }
                }

                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// Find the span of a try expression in an expression, if it exists
    fn expr_find_try(&self, expr: &crate::ast::Expr) -> Option<Span> {
        match expr {
            crate::ast::Expr::Try { span, .. } => Some(*span),
            crate::ast::Expr::Cast { expr, .. } => self.expr_find_try(expr),
            crate::ast::Expr::Dereference { expr, .. } => self.expr_find_try(expr),
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
            crate::ast::Expr::Struct {
                fields,
                generic_args: _,
                ..
            } => {
                for (_, field_expr) in fields {
                    if let Some(s) = self.expr_find_try(field_expr) {
                        return Some(s);
                    }
                }
                None
            }
            crate::ast::Expr::Array(elements, _, _) | crate::ast::Expr::Tuple(elements, _) => {
                for elem in elements {
                    if let Some(s) = self.expr_find_try(elem) {
                        return Some(s);
                    }
                }
                None
            }
            crate::ast::Expr::TupleIndex { tuple, .. } => self.expr_find_try(tuple),
            crate::ast::Expr::Dereference { expr, .. } => self.expr_find_try(expr),
            // Base cases - no try expression
            crate::ast::Expr::Int(_, _)
            | crate::ast::Expr::Float(_, _)
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
                // Skip type checking when there's an explicit type annotation
                // The infer pass will handle it with proper function type conversion
                if ty.is_none() {
                    let inferred_ty = if let Some(val_expr) = value {
                        self.analyze_expression(val_expr)?
                    } else {
                        return Err(AnalysisError::new_with_span(
                            "Variable must have either a type or an initial value",
                            span,
                        )
                        .with_module("types"));
                    };

                    // Define the variable in the symbol table
                    if let Some(ns) = names {
                        for (index, name_opt) in ns.iter().enumerate() {
                            if let Some(n) = name_opt {
                                if self.symbol_table.contains(n) {
                                    return Err(AnalysisError::new_with_span(
                                        &format!(
                                            "Variable '{}' is already declared in this scope",
                                            n
                                        ),
                                        span,
                                    )
                                    .with_module("types"));
                                }
                                self.symbol_table.define(
                                    n.clone(),
                                    destructured_binding_type(&inferred_ty, index),
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
                            )
                            .with_module("types"));
                        }
                        self.symbol_table.define(
                            name.clone(),
                            inferred_ty.clone(),
                            Visibility::Private,
                            matches!(mutability, crate::ast::Mutability::Const),
                        );
                    }
                } else {
                    // Has explicit type - still analyze the initializer so nested expressions
                    // (like try/catch) and type mismatches are validated.
                    let explicit_ty = ty.clone().unwrap();
                    let value_ty = if let Some(val_expr) = value {
                        self.validate_expr_against_type(val_expr, &explicit_ty)?;
                        Some(self.analyze_expression(val_expr)?)
                    } else {
                        None
                    };

                    // Define the variable in the symbol table
                    if let Some(ns) = names {
                        for (index, name_opt) in ns.iter().enumerate() {
                            if let Some(n) = name_opt {
                                if self.symbol_table.contains(n) {
                                    return Err(AnalysisError::new_with_span(
                                        &format!(
                                            "Variable '{}' is already declared in this scope",
                                            n
                                        ),
                                        span,
                                    )
                                    .with_module("types"));
                                }
                                self.symbol_table.define(
                                    n.clone(),
                                    destructured_binding_type(&explicit_ty, index),
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
                            )
                            .with_module("types"));
                        }
                        self.symbol_table.define(
                            name.clone(),
                            explicit_ty.clone(),
                            Visibility::Private,
                            matches!(mutability, crate::ast::Mutability::Const),
                        );
                    }

                    if let Some(value_ty) = value_ty {
                        if !self.types_compatible(&explicit_ty, &value_ty) {
                            return Err(AnalysisError::new_with_span(
                                &format!(
                                    "Type mismatch in declaration '{}': expected {}, found {}",
                                    name, explicit_ty, value_ty
                                ),
                                span,
                            )
                            .with_module("types"));
                        }
                    }
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
                    // Check if this is a member access (contains a dot)
                    if target.contains('.') {
                        // For member access like "self.i", we need to handle it specially
                        // Split by dot - the first part is the base identifier
                        let parts: Vec<&str> = target.split('.').collect();
                        if let Some(base) = parts.first() {
                            // Check if the base identifier is in the symbol table
                            // or if it's a special case like 'self' (method receiver)
                            if *base != "self" {
                                let _ = self.symbol_table.resolve(base).ok_or_else(|| {
                                    AnalysisError::new_with_span(
                                        &format!("Undefined variable '{}'", base),
                                        span,
                                    )
                                })?;
                            }
                            // For now, we skip detailed type validation for member assignments
                            // The type will be validated when analyzing the expression
                        }
                    } else {
                        // Regular variable assignment - resolve the identifier
                        let symbol_ty = self
                            .symbol_table
                            .resolve(target)
                            .map(|s| s.ty.clone())
                            .ok_or_else(|| {
                                AnalysisError::new_with_span(
                                    &format!("Undefined variable '{}'", target),
                                    span,
                                )
                                .with_module("types")
                            })?;
                        self.validate_expr_against_type(value, &symbol_ty)?;
                        let expr_ty = self.analyze_expression(value)?;
                        if !self.types_compatible(&symbol_ty, &expr_ty) {
                            return Err(AnalysisError::new_with_span(
                                &format!(
                                    "Type mismatch in assignment to '{}': expected {}, found {}",
                                    target, symbol_ty, expr_ty
                                ),
                                span,
                            )
                            .with_module("types"));
                        }
                    }
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
                capture,
                then_branch,
                else_branch,
                span,
            } => {
                let cond_ty = self.analyze_expression(condition)?;

                // Handle capture variable if present
                if let Some(cap) = capture {
                    // Enter a new scope for the capture variable
                    self.symbol_table.enter_scope();
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
                        )
                        .with_module("types"));
                    }
                }

                self.analyze_statement(then_branch)?;

                // Exit the scope if a capture variable was present
                if capture.is_some() {
                    self.symbol_table.exit_scope();
                }
                if let Some(eb) = else_branch {
                    self.analyze_statement(eb)?;
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
            crate::ast::Stmt::Break { .. } => Ok(()),
            crate::ast::Stmt::Continue { .. } => Ok(()),
        }
    }

    fn analyze_expression(&mut self, expr: &crate::ast::Expr) -> AnalysisResult<crate::ast::Type> {
        match expr {
            crate::ast::Expr::Int(_, _) => Ok(crate::ast::Type::I64),
            crate::ast::Expr::Float(_, _) => Ok(crate::ast::Type::F64),
            crate::ast::Expr::Bool(_, _) => Ok(crate::ast::Type::Bool),
            crate::ast::Expr::String(_, _) => Ok(crate::ast::Type::Array {
                size: None,
                element_type: Box::new(crate::ast::Type::U8),
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
                // Check if it's a known package
                if name == "std" || name == "io" || name == "os" {
                    return Ok(crate::ast::Type::I64); // Return placeholder for package
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
            crate::ast::Expr::Array(elements, explicit_ty, span) => {
                let mut element_type = explicit_ty.clone();

                for (index, elem) in elements.iter().enumerate() {
                    if let Some(expected_elem_ty) = element_type.as_ref() {
                        self.validate_expr_against_type(elem, expected_elem_ty)?;
                    }

                    let elem_ty = self.analyze_expression(elem)?;

                    if let Some(expected_elem_ty) = element_type.as_ref() {
                        if !self.types_compatible(expected_elem_ty, &elem_ty) {
                            return Err(AnalysisError::new_with_span(
                                &format!(
                                    "Array element #{} has type {}, expected {}",
                                    index + 1,
                                    elem_ty,
                                    expected_elem_ty
                                ),
                                span,
                            )
                            .with_module("types"));
                        }
                    } else {
                        element_type = Some(elem_ty);
                    }
                }

                Ok(crate::ast::Type::Array {
                    size: Some(elements.len()),
                    element_type: Box::new(element_type.unwrap_or(crate::ast::Type::I8)),
                })
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
                        // If types are Function, they're being checked incorrectly - skip check
                        let l_is_fn = matches!(l_ty, crate::ast::Type::Function { .. });
                        let r_is_fn = matches!(r_ty, crate::ast::Type::Function { .. });
                        if l_is_fn || r_is_fn {
                            // Skip check - type was already resolved in infer pass
                            Ok(l_ty)
                        } else if self.is_numeric(&l_ty) && self.is_numeric(&r_ty) {
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
                    crate::ast::UnaryOp::Ref => {
                        // &expr returns a pointer to expr
                        Ok(crate::ast::Type::Pointer(Box::new(e_ty)))
                    }
                }
            }
            crate::ast::Expr::Call {
                name,
                namespace,
                args,
                generic_args, // Changed from generic_args: _
                span,
            } => {
                if namespace.as_deref() == Some("io") && name == "println" {
                    for arg in args {
                        self.analyze_expression(arg)?;
                    }
                    return Ok(crate::ast::Type::Void);
                }
                // Check if it's is_null or is_not_null (built-in functions)
                if namespace.is_none() && (name == "is_null" || name == "is_not_null") {
                    // These functions take a rawptr or pointer and return bool
                    for arg in args {
                        self.analyze_expression(arg)?;
                    }
                    return Ok(crate::ast::Type::Bool);
                }
                let symbol_ty = if let Some(ns) = namespace {
                    // Try to resolve as a struct/enum method: StructName_methodname
                    let fn_name = format!("{}_{}", ns, name);
                    self.symbol_table
                        .resolve(&fn_name)
                        .map(|s| s.ty.clone())
                        .unwrap_or(crate::ast::Type::I64)
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
                match symbol_ty {
                    crate::ast::Type::Function { return_type, .. } => Ok(*return_type),
                    other => Ok(other),
                }
            }
            crate::ast::Expr::If {
                condition,
                then_branch,
                else_branch,
                capture,
                span,
            } => {
                let cond_ty = self.analyze_expression(condition)?;

                if let Some(cap) = capture {
                    self.symbol_table.enter_scope();
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
                        )
                        .with_module("types"));
                    }
                }

                let then_ty = self.analyze_expression(then_branch)?;

                if capture.is_some() {
                    self.symbol_table.exit_scope();
                }

                let else_ty = self.analyze_expression(else_branch)?;
                if self.types_compatible(&then_ty, &else_ty) {
                    Ok(then_ty)
                } else {
                    Err(AnalysisError::new_with_span(
                        format!(
                            "if and else branches must have the same type, found '{}' and '{}'",
                            then_ty, else_ty
                        )
                        .as_str(),
                        span,
                    )
                    .with_module("types"))
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
            crate::ast::Expr::MemberAccess { object, member, .. } => {
                let obj_ty = self.analyze_expression(object)?;
                if let crate::ast::Type::Custom { name, .. } = &obj_ty {
                    // Try to find if it's a field
                    if let Some(struct_def) = self.structs.get(name) {
                        if let Some(field) = struct_def.fields.iter().find(|f| &f.name == member) {
                            return Ok(field.ty.clone());
                        }
                    }
                    Ok(crate::ast::Type::I64)
                } else {
                    Ok(crate::ast::Type::I64)
                }
            }
            crate::ast::Expr::Struct {
                name,
                fields,
                generic_args: _,
                span,
            } => {
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
                    Err(AnalysisError::new_with_span(
                        &format!("Try expression requires Result type, found {}", expr_ty),
                        span,
                    )
                    .with_module("types"))
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
                    return Err(AnalysisError::new_with_span(
                        &format!(
                            "catch expression requires a Result type, expected Result<T> but found {}",
                            expr_ty
                        ),
                        span,
                    ));
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
                expr_ty.result_inner().cloned().ok_or_else(|| {
                    AnalysisError::new_with_span("Failed to get inner type from Result", span)
                })
            }
            crate::ast::Expr::Cast {
                target_type,
                expr,
                span,
            } => {
                // First analyze the expression being cast
                let _ = self.analyze_expression(expr)?;
                // Return the target type
                Ok(target_type.clone())
            }
            crate::ast::Expr::Dereference { expr, span } => {
                // Analyze the pointer expression
                let ptr_ty = self.analyze_expression(expr)?;
                // Check if it's a pointer type and return inner type
                if let crate::ast::Type::Pointer(inner) = ptr_ty {
                    Ok(inner.as_ref().clone())
                } else {
                    Err(AnalysisError::new_with_span(
                        &format!("Cannot dereference non-pointer type: {}", ptr_ty),
                        span,
                    ))
                }
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
        if left == right {
            return true;
        }

        // Check if left is an Option and right is compatible with inner type
        if let crate::ast::Type::Option(inner) = left {
            if self.types_compatible(inner, right) {
                return true;
            }
        }
        // Check if right is an Option and left is compatible with inner type
        if let crate::ast::Type::Option(inner) = right {
            if self.types_compatible(left, inner) {
                return true;
            }
        }

        // Allow compatible numeric types
        if self.is_numeric(left) && self.is_numeric(right) {
            return true;
        }

        // Allow bool to numeric and numeric to bool conversions
        if matches!(left, crate::ast::Type::Bool) && self.is_numeric(right) {
            return true;
        }
        if self.is_numeric(left) && matches!(right, crate::ast::Type::Bool) {
            return true;
        }

        false
    }

    pub fn get_symbol_table(&self) -> &SymbolTable {
        &self.symbol_table
    }
}
