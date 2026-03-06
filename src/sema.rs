use crate::ast::{Type, Visibility};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub ty: Type,
    pub visibility: Visibility,
    pub is_const: bool,
}

#[derive(Debug, Clone)]
pub struct Scope {
    pub symbols: HashMap<String, Symbol>,
    pub parent: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct SymbolTable {
    pub scopes: Vec<Scope>,
    pub current_scope: usize,
}

impl SymbolTable {
    pub fn new() -> Self {
        let global_scope = Scope {
            symbols: HashMap::new(),
            parent: None,
        };
        SymbolTable {
            scopes: vec![global_scope],
            current_scope: 0,
        }
    }

    pub fn enter_scope(&mut self) {
        let new_scope = Scope {
            symbols: HashMap::new(),
            parent: Some(self.current_scope),
        };
        self.scopes.push(new_scope);
        self.current_scope = self.scopes.len() - 1;
    }

    pub fn exit_scope(&mut self) {
        if let Some(parent) = self.scopes[self.current_scope].parent {
            self.current_scope = parent;
        }
    }

    pub fn define(&mut self, name: String, ty: Type, visibility: Visibility, is_const: bool) {
        let symbol = Symbol {
            name: name.clone(),
            ty,
            visibility,
            is_const,
        };
        self.scopes[self.current_scope].symbols.insert(name, symbol);
    }

    pub fn resolve(&self, name: &str) -> Option<&Symbol> {
        let mut scope_idx = Some(self.current_scope);
        while let Some(idx) = scope_idx {
            let scope = &self.scopes[idx];
            if let Some(symbol) = scope.symbols.get(name) {
                return Some(symbol);
            }
            scope_idx = scope.parent;
        }
        None
    }
}

// ============================================================================
// Analysis Error Types
// ============================================================================

#[derive(Debug, Clone)]
pub struct AnalysisError {
    pub message: String,
}

impl AnalysisError {
    pub fn new(message: &str) -> Self {
        AnalysisError {
            message: message.to_string(),
        }
    }
}

pub type AnalysisResult<T> = Result<T, AnalysisError>;

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
                mutability,
                ..
            } => {
                let inferred_ty = if let Some(val_expr) = value {
                    let v_ty = self.analyze_expression(val_expr)?;
                    if let Some(explicit_ty) = ty {
                        if !self.types_compatible(explicit_ty, &v_ty) {
                            return Err(AnalysisError::new(&format!(
                                "Type mismatch in variable declaration: expected {}, found {}",
                                explicit_ty, v_ty
                            )));
                        }
                    }
                    v_ty
                } else if let Some(explicit_ty) = ty {
                    explicit_ty.clone()
                } else {
                    return Err(AnalysisError::new(
                        "Variable must have either a type or an initial value",
                    ));
                };

                // Handle both single name and tuple destructuring
                if let Some(ns) = names {
                    for name_opt in ns {
                        if let Some(n) = name_opt {
                            self.symbol_table.define(
                                n.clone(),
                                inferred_ty.clone(),
                                Visibility::Private,
                                matches!(mutability, crate::ast::Mutability::Const),
                            );
                        }
                    }
                } else {
                    self.symbol_table.define(
                        name.clone(),
                        inferred_ty,
                        Visibility::Private,
                        matches!(mutability, crate::ast::Mutability::Const),
                    );
                }
                Ok(())
            }
            crate::ast::Stmt::Assign { target, value, .. } => {
                if target != "_" {
                    let symbol_ty = self
                        .symbol_table
                        .resolve(target)
                        .map(|s| s.ty.clone())
                        .ok_or_else(|| {
                            AnalysisError::new(&format!("Undefined variable '{}'", target))
                        })?;
                    let expr_ty = self.analyze_expression(value)?;
                    if !self.types_compatible(&symbol_ty, &expr_ty) {
                        return Err(AnalysisError::new(&format!(
                            "Type mismatch in assignment to '{}': expected {}, found {}",
                            target, symbol_ty, expr_ty
                        )));
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
                then_branch,
                else_branch,
                ..
            } => {
                self.analyze_expression(condition)?;
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
                    self.symbol_table
                        .define(vn.clone(), Type::I64, Visibility::Private, false);
                }
                if let Some(cv) = capture {
                    self.symbol_table
                        .define(cv.clone(), Type::I64, Visibility::Private, false);
                }
                if let Some(iv) = index_var {
                    self.symbol_table
                        .define(iv.clone(), Type::I64, Visibility::Private, false);
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
        }
    }

    fn analyze_expression(&mut self, expr: &crate::ast::Expr) -> AnalysisResult<Type> {
        match expr {
            crate::ast::Expr::Int(_, _) => Ok(Type::I64),
            crate::ast::Expr::Bool(_, _) => Ok(Type::Bool),
            crate::ast::Expr::String(_, _) => Ok(Type::Custom {
                name: "String".to_string(),
                generic_args: vec![],
                is_exported: false,
            }),
            crate::ast::Expr::Char(_, _) => Ok(Type::I8),
            crate::ast::Expr::Null(_) => Ok(Type::I64),
            crate::ast::Expr::Tuple(elements, _) => {
                let mut types = vec![];
                for elem in elements {
                    let ty = self.analyze_expression(elem)?;
                    types.push(ty);
                }
                Ok(Type::Tuple(types))
            }
            crate::ast::Expr::TupleIndex { tuple, index, .. } => {
                let tuple_ty = self.analyze_expression(tuple)?;
                if let Type::Tuple(types) = tuple_ty {
                    if *index < types.len() {
                        Ok(types[*index].clone())
                    } else {
                        Err(AnalysisError::new("Tuple index out of bounds"))
                    }
                } else {
                    Err(AnalysisError::new("Tuple index on non-tuple type"))
                }
            }
            crate::ast::Expr::Ident(name, _) => {
                if name == "_" {
                    return Ok(Type::I64);
                }
                self.symbol_table
                    .resolve(name)
                    .map(|s| s.ty.clone())
                    .ok_or_else(|| AnalysisError::new(&format!("Undefined variable '{}'", name)))
            }
            crate::ast::Expr::Array(elements, _) => {
                for elem in elements {
                    self.analyze_expression(elem)?;
                }
                Ok(Type::I64)
            }
            crate::ast::Expr::Binary {
                op, left, right, ..
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
                            Err(AnalysisError::new(
                                "Binary operation requires numeric operands",
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
                            Ok(Type::Bool)
                        } else {
                            Err(AnalysisError::new("Comparison requires compatible types"))
                        }
                    }
                    crate::ast::BinaryOp::And | crate::ast::BinaryOp::Or => {
                        if l_ty == Type::Bool && r_ty == Type::Bool {
                            Ok(Type::Bool)
                        } else {
                            Err(AnalysisError::new(
                                "Logical operation requires boolean operands",
                            ))
                        }
                    }
                    crate::ast::BinaryOp::Range => Ok(Type::I64),
                }
            }
            crate::ast::Expr::Unary { op, expr, .. } => {
                let e_ty = self.analyze_expression(expr)?;
                match op {
                    crate::ast::UnaryOp::Neg | crate::ast::UnaryOp::Pos => {
                        if self.is_numeric(&e_ty) {
                            Ok(e_ty)
                        } else {
                            Err(AnalysisError::new("Unary requires numeric operand"))
                        }
                    }
                    crate::ast::UnaryOp::Not => {
                        if e_ty == Type::Bool {
                            Ok(Type::Bool)
                        } else {
                            Err(AnalysisError::new("Logical NOT requires boolean operand"))
                        }
                    }
                }
            }
            crate::ast::Expr::Call {
                name,
                namespace,
                args,
                ..
            } => {
                if namespace.as_deref() == Some("io") && name == "println" {
                    for arg in args {
                        self.analyze_expression(arg)?;
                    }
                    return Ok(Type::Void);
                }
                let symbol_ty = if namespace.is_some() {
                    Type::I64
                } else {
                    self.symbol_table
                        .resolve(name)
                        .map(|s| s.ty.clone())
                        .ok_or_else(|| {
                            AnalysisError::new(&format!("Undefined function '{}'", name))
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
                ..
            } => {
                self.analyze_expression(condition)?;
                let then_ty = self.analyze_expression(then_branch)?;
                let else_ty = self.analyze_expression(else_branch)?;
                if self.types_compatible(&then_ty, &else_ty) {
                    Ok(then_ty)
                } else {
                    Err(AnalysisError::new(
                        "If expression branches must have compatible types",
                    ))
                }
            }
            crate::ast::Expr::Block { stmts, .. } => {
                self.symbol_table.enter_scope();
                for s in stmts {
                    self.analyze_statement(s)?;
                }
                self.symbol_table.exit_scope();
                Ok(Type::I64)
            }
            crate::ast::Expr::MemberAccess { object, .. } => {
                self.analyze_expression(object)?;
                Ok(Type::I64)
            }
            crate::ast::Expr::Struct { name, fields, .. } => {
                self.symbol_table
                    .resolve(name)
                    .map(|s| s.ty.clone())
                    .ok_or_else(|| AnalysisError::new(&format!("Undefined struct '{}'", name)))?;
                for (_, field_expr) in fields {
                    self.analyze_expression(field_expr)?;
                }
                Ok(Type::Custom {
                    name: name.clone(),
                    generic_args: vec![],
                    is_exported: false,
                })
            }
            crate::ast::Expr::Try { expr, .. } => {
                let expr_ty = self.analyze_expression(expr)?;
                if expr_ty.is_result() {
                    expr_ty
                        .result_inner()
                        .cloned()
                        .ok_or_else(|| AnalysisError::new("Try expression requires Result type"))
                } else {
                    Ok(expr_ty)
                }
            }
            crate::ast::Expr::Catch {
                expr,
                error_var,
                body,
                span: _,
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
                    self.symbol_table
                        .define(ev.unwrap(), Type::Error, Visibility::Private, false);
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

    fn is_numeric(&self, ty: &Type) -> bool {
        matches!(
            ty,
            Type::I8
                | Type::I16
                | Type::I32
                | Type::I64
                | Type::U8
                | Type::U16
                | Type::U32
                | Type::U64
        )
    }

    fn types_compatible(&self, left: &Type, right: &Type) -> bool {
        left == right || (self.is_numeric(left) && self.is_numeric(right))
    }

    pub fn get_symbol_table(&self) -> &SymbolTable {
        &self.symbol_table
    }
}

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
                    .map(|_| Type::I64)
                    .unwrap_or_else(|| ty.clone().unwrap_or(Type::I64));
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
            crate::ast::Stmt::Assign { target, value, .. } => {
                if target != "_" {
                    self.symbol_table.resolve(target).ok_or_else(|| {
                        AnalysisError::new(&format!("Undefined variable '{}'", target))
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
                    self.symbol_table
                        .define(cap.unwrap(), Type::I64, Visibility::Private, false);
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
                    self.symbol_table
                        .define(cap.unwrap(), Type::I64, Visibility::Private, false);
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
                    self.symbol_table
                        .define(vn.clone(), Type::I64, Visibility::Private, false);
                }
                if let Some(cv) = capture {
                    self.symbol_table
                        .define(cv.clone(), Type::I64, Visibility::Private, false);
                }
                if let Some(iv) = index_var {
                    self.symbol_table
                        .define(iv.clone(), Type::I64, Visibility::Private, false);
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

    fn analyze_expression(&mut self, expr: &crate::ast::Expr) -> AnalysisResult<Type> {
        match expr {
            crate::ast::Expr::Ident(name, _) => {
                if name == "_" {
                    return Ok(Type::I64);
                }
                self.symbol_table
                    .resolve(name)
                    .map(|s| s.ty.clone())
                    .ok_or_else(|| AnalysisError::new(&format!("Undefined identifier '{}'", name)))
            }
            crate::ast::Expr::Call {
                name,
                namespace,
                args,
                ..
            } => {
                if namespace.as_deref() != Some("io") || name != "println" {
                    if namespace.is_none() {
                        self.symbol_table.resolve(name).ok_or_else(|| {
                            AnalysisError::new(&format!("Undefined function '{}'", name))
                        })?;
                    }
                }
                for arg in args {
                    self.analyze_expression(arg)?;
                }
                Ok(Type::I64)
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
                    self.symbol_table
                        .define(ev.unwrap(), Type::Error, Visibility::Private, false);
                }
                self.analyze_expression(body)?;
                if error_var.is_some() {
                    self.symbol_table.exit_scope();
                }
                Ok(Type::I64)
            }
            crate::ast::Expr::Struct { name, fields, .. } => {
                self.symbol_table
                    .resolve(name)
                    .ok_or_else(|| AnalysisError::new(&format!("Undefined struct '{}'", name)))?;
                for (_, field_expr) in fields {
                    self.analyze_expression(field_expr)?;
                }
                Ok(Type::I64)
            }
            crate::ast::Expr::Binary { left, right, .. } => {
                self.analyze_expression(left)?;
                self.analyze_expression(right)?;
                Ok(Type::I64)
            }
            crate::ast::Expr::Unary { expr, .. } => {
                self.analyze_expression(expr)?;
                Ok(Type::I64)
            }
            crate::ast::Expr::MemberAccess { object, .. } => {
                self.analyze_expression(object)?;
                Ok(Type::I64)
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
                Ok(Type::I64)
            }
            crate::ast::Expr::Block { stmts, .. } => {
                self.symbol_table.enter_scope();
                for s in stmts {
                    self.analyze_statement(s)?;
                }
                self.symbol_table.exit_scope();
                Ok(Type::I64)
            }
            crate::ast::Expr::Try { expr, .. } => {
                self.analyze_expression(expr)?;
                Ok(Type::I64)
            }
            _ => Ok(Type::I64),
        }
    }

    pub fn get_symbol_table(&self) -> &SymbolTable {
        &self.symbol_table
    }
}

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
            crate::ast::Stmt::Assign { target, .. } => {
                if target != "_" {
                    let is_const = self
                        .symbol_table
                        .resolve(target)
                        .map(|s| s.is_const)
                        .ok_or_else(|| {
                            AnalysisError::new(&format!("Undefined variable '{}'", target))
                        })?;
                    if is_const {
                        return Err(AnalysisError::new(&format!(
                            "Cannot reassign constant variable '{}'",
                            target
                        )));
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
                    self.symbol_table
                        .define(cap.unwrap(), Type::I64, Visibility::Private, false);
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
                    self.symbol_table
                        .define(cap.unwrap(), Type::I64, Visibility::Private, false);
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
                    self.symbol_table
                        .define(vn.clone(), Type::I64, Visibility::Private, false);
                }
                if let Some(cv) = capture {
                    self.symbol_table
                        .define(cv.clone(), Type::I64, Visibility::Private, false);
                }
                if let Some(iv) = index_var {
                    self.symbol_table
                        .define(iv.clone(), Type::I64, Visibility::Private, false);
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

// ============================================================================
// Main Semantic Analyzer
// Orchestrates all analysis passes
// ============================================================================

pub struct SemanticAnalyzer {
    pub symbol_table: SymbolTable,
}

impl SemanticAnalyzer {
    pub fn new() -> Self {
        SemanticAnalyzer {
            symbol_table: SymbolTable::new(),
        }
    }

    pub fn analyze(&mut self, program: &crate::ast::Program) -> Result<(), String> {
        // Pass 1: Collect and validate global definitions
        let mut global_analyzer = GlobalDefinitionsAnalyzer::new();
        global_analyzer.analyze(program).map_err(|e| e.message)?;

        // Pass 2: Type analysis
        let symbol_table = global_analyzer.get_symbol_table().clone();
        let mut type_analyzer = TypeAnalyzer::new(symbol_table);
        type_analyzer.analyze(program).map_err(|e| e.message)?;

        // Pass 3: Symbol resolution
        let symbol_table = type_analyzer.get_symbol_table().clone();
        let mut symbol_resolver = SymbolResolver::new(symbol_table);
        symbol_resolver.analyze(program).map_err(|e| e.message)?;

        // Pass 4: Mutability analysis
        let symbol_table = symbol_resolver.get_symbol_table().clone();
        let mut mutability_analyzer = MutabilityAnalyzer::new(symbol_table);
        mutability_analyzer
            .analyze(program)
            .map_err(|e| e.message)?;

        // Store final symbol table
        self.symbol_table = mutability_analyzer.get_symbol_table().clone();

        Ok(())
    }

    pub fn get_symbol_table(&self) -> &SymbolTable {
        &self.symbol_table
    }
}

impl Default for SemanticAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}
