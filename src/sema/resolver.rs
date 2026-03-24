use crate::ast::{Span, Visibility};
use crate::sema::error::{AnalysisError, AnalysisResult};
use crate::sema::symbol::SymbolTable;

// ============================================================================
// Analysis Pass 3: Symbol Resolver
// Resolves variable/function references and checks scope correctness
// ============================================================================

pub struct SymbolResolver {
    symbol_table: SymbolTable,
    // Reference to structs for method visibility checks
    structs: std::collections::HashMap<String, crate::ast::StructDef>,
    enums: std::collections::HashMap<String, crate::ast::EnumDef>,
    // Current struct context (if we're inside a struct method)
    current_struct: Option<String>,
}

fn destructured_binding_type(aggregate_ty: &crate::ast::Type, index: usize) -> crate::ast::Type {
    match aggregate_ty {
        crate::ast::Type::Tuple(types) => types.get(index).cloned().unwrap_or(crate::ast::Type::I64),
        _ => aggregate_ty.clone(),
    }
}

impl SymbolResolver {
    pub fn new(
        symbol_table: SymbolTable,
        structs: std::collections::HashMap<String, crate::ast::StructDef>,
        enums: std::collections::HashMap<String, crate::ast::EnumDef>,
    ) -> Self {
        SymbolResolver {
            symbol_table,
            structs,
            enums,
            current_struct: None,
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
                self.analyze_struct(s)?;
            }
        }
        // Analyze enum methods
        for e in &program.enums {
            if e.generic_params.is_empty() {
                self.analyze_enum(e)?;
            }
        }
        Ok(())
    }

    fn analyze_struct(&mut self, s: &crate::ast::StructDef) -> AnalysisResult<()> {
        // Set current struct context
        let previous_struct = self.current_struct.clone();
        self.current_struct = Some(s.name.clone());

        for method in &s.methods {
            self.analyze_function(method)?;
        }

        // Restore previous struct context
        self.current_struct = previous_struct;
        Ok(())
    }

    fn analyze_enum(&mut self, e: &crate::ast::EnumDef) -> AnalysisResult<()> {
        // Set current enum context (enum methods work similar to struct methods)
        let previous_struct = self.current_struct.clone();
        self.current_struct = Some(e.name.clone());

        for method in &e.methods {
            self.analyze_function(method)?;
        }

        // Restore previous struct context
        self.current_struct = previous_struct;
        Ok(())
    }

    /// Check if a method call is allowed based on visibility
    /// Returns Ok if the call is allowed, Err if not
    ///
    /// Private methods (without pub) can only be called from:
    /// 1. Same struct's methods
    /// 2. Other functions in the same file
    ///
    /// Since all analysis is done for a single file at a time, we allow calls
    /// from any context within the same analysis pass. External file calls
    /// would need module-level visibility tracking which is not implemented yet.
    fn check_method_visibility(
        &self,
        struct_name: &str,
        method_name: &str,
        _span: Span,
    ) -> AnalysisResult<()> {
        // First, check if it's a struct
        if let Some(struct_def) = self.structs.get(struct_name) {
            // Find the method
            if let Some(method) = struct_def.methods.iter().find(|m| &m.name == method_name) {
                // Check if method is public
                if method.visibility == Visibility::Public {
                    return Ok(());
                }

                // Method is private - check if we're calling from the same struct
                if let Some(ref current) = self.current_struct {
                    if current == struct_name {
                        // Calling from same struct's method - allowed
                        return Ok(());
                    }
                }

                // For now, allow calls from anywhere in the same file
                // (all functions are analyzed together, so they're all "in file")
                // In a full implementation, we would track file/module boundaries
                return Ok(());
            }
        }

        // Check if it's an enum
        if let Some(enum_def) = self.enums.get(struct_name) {
            // Find the method
            if let Some(method) = enum_def.methods.iter().find(|m| &m.name == method_name) {
                // Check if method is public
                if method.visibility == Visibility::Public {
                    return Ok(());
                }

                // Method is private - check if we're calling from the same enum
                if let Some(ref current) = self.current_struct {
                    if current == struct_name {
                        // Calling from same enum's method - allowed
                        return Ok(());
                    }
                }

                // For now, allow calls from anywhere in the same file
                return Ok(());
            }
        }

        // Method not found - let other code handle it (will get undefined error later)
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
                let value_ty = if let Some(val_expr) = value {
                    Some(self.analyze_expression(val_expr)?)
                } else {
                    None
                };
                let inferred_ty = if let Some(explicit_ty) = ty {
                    explicit_ty.clone()
                } else if let Some(value_ty) = value_ty {
                    value_ty
                } else {
                    crate::ast::Type::I64
                };
                if let Some(ns) = names {
                    for (index, name_opt) in ns.iter().enumerate() {
                        if let Some(n) = name_opt {
                            self.symbol_table.define(
                                n.clone(),
                                destructured_binding_type(&inferred_ty, index),
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
                    // Check if this is a member access (contains a dot)
                    if target.contains('.') {
                        // For member access like "self.i", we need to handle it specially
                        // Split by dot - the first part is the base identifier
                        let parts: Vec<&str> = target.split('.').collect();
                        if let Some(base) = parts.first() {
                            // Check if the base identifier is in the symbol table
                            // or if it's a special case like 'self' (method receiver)
                            if *base != "self" {
                                self.symbol_table.resolve(base).ok_or_else(|| {
                                    AnalysisError::new_with_span(
                                        &format!("Undefined variable '{}'", base),
                                        span,
                                    )
                                    .with_module("resolver")
                                })?;
                            }
                            // For now, we skip detailed member validation
                            // The expression analyzer will handle proper member access validation
                        }
                    } else {
                        // Regular variable assignment - resolve the identifier
                        self.symbol_table.resolve(target).ok_or_else(|| {
                            AnalysisError::new_with_span(
                                &format!("Undefined variable '{}'", target),
                                span,
                            )
                            .with_module("resolver")
                        })?;
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
            crate::ast::Stmt::For {
                label: _,
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
            crate::ast::Stmt::Defer { stmt, .. } => {
                self.analyze_statement(stmt)?;
                Ok(())
            }
            crate::ast::Stmt::DeferBang { stmt, .. } => {
                // DeferBang is similar to Defer but only executes on error
                self.analyze_statement(stmt)?;
                Ok(())
            }
            crate::ast::Stmt::Break { .. } => {
                // Break statement - no special analysis needed at resolve phase
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
                        .with_module("resolver")
                    })
            }
            crate::ast::Expr::Call {
                name,
                namespace,
                args: _,
                generic_args: _,
                span,
            } => {
                // Check if it's io.println (special case)
                if namespace.as_deref() == Some("io") && name == "println" {
                    // io.println returns void
                    return Ok(crate::ast::Type::Void);
                }

                // Check if it's is_null or is_not_null (built-in functions)
                if namespace.is_none() && (name == "is_null" || name == "is_not_null") {
                    // These functions take a rawptr or pointer and return bool
                    return Ok(crate::ast::Type::Bool);
                }

                // For other calls, we'd need to look up the function type
                // For now, handle namespace calls (like Config.parse)
                if let Some(ns) = namespace {
                    // This is a namespace call - try to find the struct method
                    // Check method visibility
                    self.check_method_visibility(&ns, name, *span)?;

                    // Try to resolve as a struct/enum method: StructName_methodname
                    let fn_name = format!("{}_{}", ns, name);
                    Ok(self
                        .symbol_table
                        .resolve(&fn_name)
                        .map(|s| s.ty.clone())
                        .unwrap_or(crate::ast::Type::I64))
                } else {
                    // Regular function call
                    // Try to resolve the function
                    let symbol_ty = if let Some(symbol) = self.symbol_table.resolve(name) {
                        symbol.ty.clone()
                    } else {
                        return Err(AnalysisError::new_with_span(
                            &format!("Undefined function '{}'", name),
                            span,
                        )
                        .with_module("resolver"));
                    };
                    match symbol_ty {
                        crate::ast::Type::Function { return_type, .. } => Ok(*return_type),
                        other => Ok(other),
                    }
                }
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
            crate::ast::Expr::Struct { name, fields, generic_args: _, .. } => {
                self.symbol_table.resolve(name).ok_or_else(|| {
                    AnalysisError::new(&format!("Undefined struct '{}'", name))
                        .with_module("resolver")
                })?;
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
            crate::ast::Expr::MemberAccess {
                object, member: _, ..
            } => {
                let _obj_ty = self.analyze_expression(object)?;
                // For resolver, we mainly just check that the object is valid
                // Detailed member resolution usually happens in infer_types
                Ok(crate::ast::Type::I64)
            }
            crate::ast::Expr::If {
                condition,
                then_branch,
                else_branch,
                capture,
                ..
            } => {
                self.analyze_expression(condition)?;
                if let Some(cap) = capture {
                    let cond_ty = self.analyze_expression(condition)?;
                    self.symbol_table.enter_scope();
                    let capture_ty = if let crate::ast::Type::Option(inner_ty) = cond_ty {
                        *inner_ty
                    } else {
                        crate::ast::Type::I64
                    };
                    self.symbol_table
                        .define(cap.clone(), capture_ty, Visibility::Private, false);
                    self.analyze_expression(then_branch)?;
                    self.symbol_table.exit_scope();
                } else {
                    self.analyze_expression(condition)?;
                    self.analyze_expression(then_branch)?;
                }
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
