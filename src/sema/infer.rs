//! # Type Inference Engine
//!
//! This module provides type inference for the AST, producing a type-annotated AST
//! where every expression has its inferred type explicitly stored.

use crate::ast::Visibility;
use crate::ast::{BinaryOp, Expr, FnDef, FnParam, Program, Span, Stmt, Type, UnaryOp};
use crate::sema::error::{AnalysisError, AnalysisResult};
use crate::sema::symbol::SymbolTable;
use std::collections::{HashMap, HashSet};

// ============================================================================
// Type-Annotated AST Nodes
// ============================================================================

/// Type-annotated expression with its inferred type
#[derive(Debug, Clone)]
pub struct TypedExpr {
    pub expr: TypedExprKind,
    pub ty: Type,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum TypedExprKind {
    /// Integer literal (i64)
    Int(i64),
    /// Float literal (f64)
    Float(f64),
    /// Boolean literal
    Bool(bool),
    /// String literal
    String(String),
    /// Character literal
    Char(char),
    /// Null literal
    Null,
    /// Tuple literal
    Tuple(Vec<TypedExpr>),
    /// Tuple index access
    TupleIndex { tuple: Box<TypedExpr>, index: usize },
    /// Variable identifier
    Ident(String),
    /// Array literal
    Array(Vec<TypedExpr>),
    /// Binary operation
    Binary {
        op: BinaryOp,
        left: Box<TypedExpr>,
        right: Box<TypedExpr>,
    },
    /// Unary operation
    Unary { op: UnaryOp, expr: Box<TypedExpr> },
    /// Function call
    Call {
        name: String,
        namespace: Option<String>,
        args: Vec<TypedExpr>,
    },
    /// If expression
    If {
        condition: Box<TypedExpr>,
        /// Optional capture variable
        capture: Option<String>,
        then_branch: Box<TypedExpr>,
        else_branch: Box<TypedExpr>,
    },
    /// Block expression
    Block { stmts: Vec<TypedStmt> },
    /// Member access
    MemberAccess {
        object: Box<TypedExpr>,
        member: String,
        kind: crate::ast::MemberAccessKind,
    },
    /// Struct literal
    Struct {
        name: String,
        fields: Vec<(String, TypedExpr)>,
    },
    /// Try expression
    Try { expr: Box<TypedExpr> },
    /// Catch expression
    Catch {
        expr: Box<TypedExpr>,
        /// Optional capture variable for the error
        error_var: Option<String>,
        body: Box<TypedExpr>,
    },
}

impl TypedExpr {
    /// Create a typed expression from an AST expression
    pub fn from_ast(expr: &Expr, inferrer: &mut TypeInferrer) -> AnalysisResult<TypedExpr> {
        inferrer.infer_expr(expr)
    }

    /// Get the span of this expression
    pub fn span(&self) -> Span {
        self.span
    }
}

/// Type-annotated statement
#[derive(Debug, Clone)]
pub struct TypedStmt {
    pub stmt: TypedStmtKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum TypedStmtKind {
    /// Expression statement
    Expr { expr: TypedExpr },
    /// Import statement
    Import {
        packages: Vec<(Option<String>, String)>,
    },
    /// Variable declaration
    Let {
        name: String,
        names: Option<Vec<Option<String>>>,
        ty: Type,
        value: Option<TypedExpr>,
        is_const: bool,
    },
    /// Assignment statement
    Assign { target: String, value: TypedExpr },
    /// Return statement
    Return { value: Option<TypedExpr> },
    /// Block statement
    Block { stmts: Vec<TypedStmt> },
    /// If statement
    If {
        condition: TypedExpr,
        capture: Option<String>,
        then_branch: Box<TypedStmt>,
        else_branch: Option<Box<TypedStmt>>,
    },
    /// While loop
    While {
        condition: TypedExpr,
        capture: Option<String>,
        body: Box<TypedStmt>,
    },
    /// For loop
    For {
        label: Option<String>,
        var_name: Option<String>,
        iterable: TypedExpr,
        capture: Option<String>,
        index_var: Option<String>,
        body: Box<TypedStmt>,
    },
    /// Switch statement
    Switch {
        condition: TypedExpr,
        cases: Vec<TypedSwitchCase>,
    },
    /// Defer statement
    Defer { stmt: Box<TypedStmt> },
    /// Defer! statement
    DeferBang { stmt: Box<TypedStmt> },
    /// Break statement
    Break { label: Option<String> },
}

#[derive(Debug, Clone)]
pub struct TypedSwitchCase {
    pub patterns: Vec<TypedExpr>,
    pub capture: Option<String>,
    pub body: TypedStmt,
}

/// Type-annotated function definition
#[derive(Debug, Clone)]
pub struct TypedFnDef {
    pub name: String,
    pub visibility: Visibility,
    pub params: Vec<TypedFnParam>,
    pub return_ty: Type,
    pub body: Vec<TypedStmt>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct TypedFnParam {
    pub name: String,
    pub ty: Type,
}

/// Type-annotated struct definition
#[derive(Debug, Clone)]
pub struct TypedStructDef {
    pub name: String,
    pub fields: Vec<crate::ast::StructField>,
    pub methods: Vec<TypedFnDef>,
    pub visibility: Visibility,
    pub generic_params: Vec<String>,
    pub span: Span,
}

/// Type-annotated enum definition
#[derive(Debug, Clone)]
pub struct TypedEnumDef {
    pub name: String,
    pub variants: Vec<crate::ast::EnumVariant>,
    pub methods: Vec<TypedFnDef>,
    pub visibility: Visibility,
    pub generic_params: Vec<String>,
    pub span: Span,
}

/// Type-annotated error definition
#[derive(Debug, Clone)]
pub struct TypedErrorDef {
    pub name: String,
    pub variants: Vec<crate::ast::ErrorVariant>,
    pub union_types: Option<Vec<Type>>,
    pub visibility: Visibility,
    pub span: Span,
}

/// Type-annotated program
#[derive(Debug, Clone)]
pub struct TypedProgram {
    pub functions: Vec<TypedFnDef>,
    pub external_functions: Vec<TypedFnDef>,
    pub structs: Vec<TypedStructDef>,
    pub enums: Vec<TypedEnumDef>,
    pub errors: Vec<TypedErrorDef>,
    pub imports: Vec<(Option<String>, String)>,
}

// ============================================================================
// Type Inference Engine
// ============================================================================

/// Type inference engine that traverses the AST and infers types
pub struct TypeInferrer {
    symbol_table: SymbolTable,
    structs: HashMap<String, crate::ast::StructDef>,
    enums: HashMap<String, crate::ast::EnumDef>,
    errors: HashMap<String, crate::ast::ErrorDef>,
}

impl TypeInferrer {
    /// Create a new type inferrer
    pub fn new(symbol_table: SymbolTable) -> Self {
        TypeInferrer {
            symbol_table,
            structs: HashMap::new(),
            enums: HashMap::new(),
            errors: HashMap::new(),
        }
    }

    /// Infer types for an entire program
    pub fn infer_program(&mut self, program: &Program) -> AnalysisResult<TypedProgram> {
        // Populate structs, enums and errors maps for exhaustiveness checking and member access refinement
        for s in &program.structs {
            self.structs.insert(s.name.clone(), s.clone());
        }
        for e in &program.enums {
            self.enums.insert(e.name.clone(), e.clone());
        }
        for e in &program.errors {
            self.errors.insert(e.name.clone(), e.clone());
        }

        let mut functions = Vec::new();
        for f in &program.functions {
            functions.push(self.infer_fn(f)?);
        }

        let mut structs = Vec::new();
        for s in &program.structs {
            structs.push(self.infer_struct(s)?);
        }

        let mut enums = Vec::new();
        for e in &program.enums {
            enums.push(self.infer_enum(e)?);
        }

        let mut errors = Vec::new();
        for e in &program.errors {
            errors.push(self.infer_error(e)?);
        }

        Ok(TypedProgram {
            functions,
            external_functions: Vec::new(), // TODO: Handle external functions
            structs,
            enums,
            errors,
            imports: program.imports.clone(),
        })
    }

    /// Infer types for a struct definition
    fn infer_struct(&mut self, s: &crate::ast::StructDef) -> AnalysisResult<TypedStructDef> {
        let mut methods = Vec::new();
        for m in &s.methods {
            methods.push(self.infer_fn(m)?);
        }

        Ok(TypedStructDef {
            name: s.name.clone(),
            fields: s.fields.clone(),
            methods,
            visibility: s.visibility,
            generic_params: s.generic_params.clone(),
            span: s.span,
        })
    }

    /// Infer types for an enum definition
    fn infer_enum(&mut self, e: &crate::ast::EnumDef) -> AnalysisResult<TypedEnumDef> {
        let mut methods = Vec::new();
        for m in &e.methods {
            methods.push(self.infer_fn(m)?);
        }

        Ok(TypedEnumDef {
            name: e.name.clone(),
            variants: e.variants.clone(),
            methods,
            visibility: e.visibility,
            generic_params: e.generic_params.clone(),
            span: e.span,
        })
    }

    /// Infer types for an error definition
    fn infer_error(&mut self, e: &crate::ast::ErrorDef) -> AnalysisResult<TypedErrorDef> {
        Ok(TypedErrorDef {
            name: e.name.clone(),
            variants: e.variants.clone(),
            union_types: e.union_types.clone(),
            visibility: e.visibility,
            span: e.span,
        })
    }

    /// Infer types for a function definition
    fn infer_fn(&mut self, f: &FnDef) -> AnalysisResult<TypedFnDef> {
        // Enter function scope and add parameters
        self.symbol_table.enter_scope();

        for param in &f.params {
            self.symbol_table.define(
                param.name.clone(),
                param.ty.clone(),
                Visibility::Private,
                false,
            );
        }

        // Infer types for the function body
        let mut body = Vec::new();
        for stmt in &f.body {
            let typed_stmt = self.infer_stmt(stmt)?;
            body.push(typed_stmt);
        }

        self.symbol_table.exit_scope();

        Ok(TypedFnDef {
            name: f.name.clone(),
            visibility: f.visibility,
            params: f
                .params
                .iter()
                .map(|p| TypedFnParam {
                    name: p.name.clone(),
                    ty: p.ty.clone(),
                })
                .collect(),
            return_ty: f.return_ty.clone(),
            body,
            span: f.span,
        })
    }

    /// Infer types for a statement
    fn infer_stmt(&mut self, stmt: &Stmt) -> AnalysisResult<TypedStmt> {
        // Extract span from the statement
        let span = match stmt {
            Stmt::Expr { span, .. } => *span,
            Stmt::Import { span, .. } => *span,
            Stmt::Let { span, .. } => *span,
            Stmt::Assign { span, .. } => *span,
            Stmt::Return { span, .. } => *span,
            Stmt::Block { span, .. } => *span,
            Stmt::If { span, .. } => *span,
            Stmt::For { span, .. } => *span,
            Stmt::Switch { span, .. } => *span,
            Stmt::Defer { span, .. } => *span,
            Stmt::DeferBang { span, .. } => *span,
            Stmt::Break { span, .. } => *span,
        };

        match stmt {
            Stmt::Expr { expr, span: _ } => {
                let typed_expr = self.infer_expr(expr)?;
                Ok(TypedStmt {
                    stmt: TypedStmtKind::Expr { expr: typed_expr },
                    span,
                })
            }
            Stmt::Import { packages, span: _ } => Ok(TypedStmt {
                stmt: TypedStmtKind::Import {
                    packages: packages.clone(),
                },
                span,
            }),
            Stmt::Let {
                name,
                names,
                ty,
                value,
                visibility: _,
                span,
                mutability,
            } => {
                let inferred_ty = if let Some(explicit_ty) = ty {
                    explicit_ty.clone()
                } else if let Some(val_expr) = value {
                    let typed_val = self.infer_expr(val_expr)?;
                    typed_val.ty.clone()
                } else {
                    return Err(AnalysisError::new_with_span(
                        "Variable must have either a type or an initial value",
                        span,
                    )
                    .with_module("infer"));
                };

                // Define the variable in the symbol table
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
                        inferred_ty.clone(),
                        Visibility::Private,
                        matches!(mutability, crate::ast::Mutability::Const),
                    );
                }

                let typed_value = if let Some(val_expr) = value {
                    Some(self.infer_expr(val_expr)?)
                } else {
                    None
                };

                Ok(TypedStmt {
                    stmt: TypedStmtKind::Let {
                        name: name.clone(),
                        names: names.clone(),
                        ty: inferred_ty,
                        value: typed_value,
                        is_const: matches!(mutability, crate::ast::Mutability::Const),
                    },
                    span: *span,
                })
            }
            Stmt::Assign {
                target,
                value,
                op: _,
                span,
            } => {
                let typed_value = self.infer_expr(value)?;
                Ok(TypedStmt {
                    stmt: TypedStmtKind::Assign {
                        target: target.clone(),
                        value: typed_value,
                    },
                    span: *span,
                })
            }
            Stmt::Return { value, span } => {
                let typed_value = if let Some(val_expr) = value {
                    Some(self.infer_expr(val_expr)?)
                } else {
                    None
                };
                Ok(TypedStmt {
                    stmt: TypedStmtKind::Return { value: typed_value },
                    span: *span,
                })
            }
            Stmt::Block { stmts, span } => {
                self.symbol_table.enter_scope();
                let mut typed_stmts = Vec::new();
                for s in stmts {
                    typed_stmts.push(self.infer_stmt(s)?);
                }
                self.symbol_table.exit_scope();
                Ok(TypedStmt {
                    stmt: TypedStmtKind::Block { stmts: typed_stmts },
                    span: *span,
                })
            }
            Stmt::If {
                condition,
                capture,
                then_branch,
                else_branch,
                span,
            } => {
                let typed_condition = self.infer_expr(condition)?;

                // Handle capture variable
                if let Some(cap) = capture {
                    self.symbol_table.enter_scope();
                    // If condition is an optional type, the capture gets the inner type
                    if let Type::Option(inner_ty) = typed_condition.ty.clone() {
                        self.symbol_table.define(
                            cap.clone(),
                            *inner_ty,
                            Visibility::Private,
                            false,
                        );
                    }
                }

                let typed_then = self.infer_stmt(then_branch)?;

                if capture.is_some() {
                    self.symbol_table.exit_scope();
                }

                let typed_else = if let Some(eb) = else_branch {
                    Some(Box::new(self.infer_stmt(eb)?))
                } else {
                    None
                };

                Ok(TypedStmt {
                    stmt: TypedStmtKind::If {
                        condition: typed_condition,
                        capture: capture.clone(),
                        then_branch: Box::new(typed_then),
                        else_branch: typed_else,
                    },
                    span: *span,
                })
            }
            Stmt::For {
                label,
                var_name,
                iterable,
                capture,
                index_var,
                body,
                span,
            } => {
                let typed_iterable = self.infer_expr(iterable)?;

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

                let typed_body = self.infer_stmt(body)?;
                self.symbol_table.exit_scope();

                Ok(TypedStmt {
                    stmt: TypedStmtKind::For {
                        label: label.clone(),
                        var_name: var_name.clone(),
                        iterable: typed_iterable,
                        capture: capture.clone(),
                        index_var: index_var.clone(),
                        body: Box::new(typed_body),
                    },
                    span: *span,
                })
            }
            Stmt::Switch {
                condition,
                cases,
                span,
            } => {
                let typed_condition = self.infer_expr(condition)?;
                let mut typed_cases = Vec::new();

                for case in cases {
                    let mut typed_patterns = Vec::new();
                    for pattern in &case.patterns {
                        typed_patterns.push(self.infer_expr(pattern)?);
                    }

                    let typed_body = self.infer_stmt(&case.body)?;

                    typed_cases.push(TypedSwitchCase {
                        patterns: typed_patterns,
                        capture: case.capture.clone(),
                        body: typed_body,
                    });
                }

                // Check exhaustiveness
                self.check_switch_exhaustiveness(&typed_condition.ty, &typed_cases, span)?;

                Ok(TypedStmt {
                    stmt: TypedStmtKind::Switch {
                        condition: typed_condition,
                        cases: typed_cases,
                    },
                    span: *span,
                })
            }
            Stmt::Defer { stmt, span } => {
                let typed_stmt = self.infer_stmt(stmt)?;
                Ok(TypedStmt {
                    stmt: TypedStmtKind::Defer {
                        stmt: Box::new(typed_stmt),
                    },
                    span: *span,
                })
            }
            Stmt::DeferBang { stmt, span } => {
                let typed_stmt = self.infer_stmt(stmt)?;
                Ok(TypedStmt {
                    stmt: TypedStmtKind::DeferBang {
                        stmt: Box::new(typed_stmt),
                    },
                    span: *span,
                })
            }
            Stmt::Break { label, span } => Ok(TypedStmt {
                stmt: TypedStmtKind::Break {
                    label: label.clone(),
                },
                span: *span,
            }),
        }
    }

    /// Infer the type of an expression
    fn infer_expr(&mut self, expr: &Expr) -> AnalysisResult<TypedExpr> {
        // Extract span from the expression
        let span = match expr {
            Expr::Int(_, span) => *span,
            Expr::Float(_, span) => *span,
            Expr::Bool(_, span) => *span,
            Expr::String(_, span) => *span,
            Expr::Char(_, span) => *span,
            Expr::Null(span) => *span,
            Expr::Tuple(_, span) => *span,
            Expr::TupleIndex { span, .. } => *span,
            Expr::Ident(_, span) => *span,
            Expr::Array(_, span) => *span,
            Expr::Binary { span, .. } => *span,
            Expr::Unary { span, .. } => *span,
            Expr::Call { span, .. } => *span,
            Expr::If { span, .. } => *span,
            Expr::Block { span, .. } => *span,
            Expr::MemberAccess { span, .. } => *span,
            Expr::Struct { span, .. } => *span,
            Expr::Try { span, .. } => *span,
            Expr::Catch { span, .. } => *span,
        };

        match expr {
            Expr::Int(value, _) => Ok(TypedExpr {
                expr: TypedExprKind::Int(*value),
                ty: Type::I64,
                span,
            }),
            Expr::Float(value, _) => Ok(TypedExpr {
                expr: TypedExprKind::Float(*value),
                ty: Type::F64,
                span,
            }),
            Expr::Bool(value, _) => Ok(TypedExpr {
                expr: TypedExprKind::Bool(*value),
                ty: Type::Bool,
                span,
            }),
            Expr::String(value, _) => Ok(TypedExpr {
                expr: TypedExprKind::String(value.clone()),
                ty: Type::Custom {
                    name: "String".to_string(),
                    generic_args: vec![],
                    is_exported: false,
                },
                span,
            }),
            Expr::Char(value, _) => Ok(TypedExpr {
                expr: TypedExprKind::Char(*value),
                ty: Type::I8,
                span,
            }),
            Expr::Null(_) => Ok(TypedExpr {
                expr: TypedExprKind::Null,
                ty: Type::Option(Box::new(Type::I64)),
                span,
            }),
            Expr::Tuple(elements, _) => {
                let mut typed_elements = Vec::new();
                for elem in elements {
                    typed_elements.push(self.infer_expr(elem)?);
                }
                let ty = Type::Tuple(typed_elements.iter().map(|e| e.ty.clone()).collect());
                Ok(TypedExpr {
                    expr: TypedExprKind::Tuple(typed_elements),
                    ty,
                    span,
                })
            }
            Expr::TupleIndex { tuple, index, span } => {
                let typed_tuple = self.infer_expr(tuple)?;
                if let Type::Tuple(types) = &typed_tuple.ty {
                    if *index < types.len() {
                        let ty = types[*index].clone();
                        Ok(TypedExpr {
                            expr: TypedExprKind::TupleIndex {
                                tuple: Box::new(typed_tuple),
                                index: *index,
                            },
                            ty,
                            span: *span,
                        })
                    } else {
                        Err(
                            AnalysisError::new_with_span("Tuple index out of bounds", span)
                                .with_module("infer"),
                        )
                    }
                } else {
                    Err(
                        AnalysisError::new_with_span("Tuple index on non-tuple type", span)
                            .with_module("infer"),
                    )
                }
            }
            Expr::Ident(name, span) => {
                if name == "_" {
                    return Ok(TypedExpr {
                        expr: TypedExprKind::Ident(name.clone()),
                        ty: Type::I64,
                        span: *span,
                    });
                }

                let ty = self
                    .symbol_table
                    .resolve(name)
                    .map(|s| s.ty.clone())
                    .ok_or_else(|| {
                        AnalysisError::new_with_span(
                            &format!("Undefined variable '{}'", name),
                            span,
                        )
                        .with_module("infer")
                    })?;

                Ok(TypedExpr {
                    expr: TypedExprKind::Ident(name.clone()),
                    ty,
                    span: *span,
                })
            }
            Expr::Array(elements, _) => {
                let mut typed_elements = Vec::new();
                for elem in elements {
                    typed_elements.push(self.infer_expr(elem)?);
                }
                // Determine the element type from the typed elements
                let element_type = if typed_elements.is_empty() {
                    // Empty array - default to i8
                    Type::I8
                } else {
                    // Use the type of the first element (assuming all elements have the same type)
                    typed_elements[0].ty.clone()
                };
                // Create array type with size and element type
                let array_ty = Type::Array {
                    size: Some(typed_elements.len()),
                    element_type: Box::new(element_type),
                };
                Ok(TypedExpr {
                    expr: TypedExprKind::Array(typed_elements),
                    ty: array_ty,
                    span,
                })
            }
            Expr::Binary {
                op,
                left,
                right,
                span,
            } => {
                let typed_left = self.infer_expr(left)?;
                let typed_right = self.infer_expr(right)?;

                let ty = self.infer_binary_op_type(op, &typed_left.ty, &typed_right.ty, span)?;

                Ok(TypedExpr {
                    expr: TypedExprKind::Binary {
                        op: *op,
                        left: Box::new(typed_left),
                        right: Box::new(typed_right),
                    },
                    ty,
                    span: *span,
                })
            }
            Expr::Unary { op, expr, span } => {
                let typed_expr = self.infer_expr(expr)?;
                let ty = self.infer_unary_op_type(op, &typed_expr.ty, span)?;

                Ok(TypedExpr {
                    expr: TypedExprKind::Unary {
                        op: *op,
                        expr: Box::new(typed_expr),
                    },
                    ty,
                    span: *span,
                })
            }
            Expr::Call {
                name,
                namespace,
                args,
                span,
            } => {
                let mut typed_args = Vec::new();
                for arg in args {
                    typed_args.push(self.infer_expr(arg)?);
                }

                // Check for special cases
                if namespace.as_deref() == Some("io") && name == "println" {
                    return Ok(TypedExpr {
                        expr: TypedExprKind::Call {
                            name: name.clone(),
                            namespace: namespace.clone(),
                            args: typed_args,
                        },
                        ty: Type::Void,
                        span: *span,
                    });
                }

                // Try to resolve function return type
                let fn_name = if let Some(ns) = namespace {
                    format!("{}_{}", ns, name)
                } else {
                    name.clone()
                };

                let ty = self
                    .symbol_table
                    .resolve(&fn_name)
                    .map(|s| s.ty.clone())
                    .unwrap_or(Type::I64); // Default to i64 if not found

                Ok(TypedExpr {
                    expr: TypedExprKind::Call {
                        name: name.clone(),
                        namespace: namespace.clone(),
                        args: typed_args,
                    },
                    ty,
                    span: *span,
                })
            }
            Expr::If {
                condition,
                capture,
                then_branch,
                else_branch,
                span,
            } => {
                let typed_condition = self.infer_expr(condition)?;

                // Handle capture variable
                if let Some(cap) = capture {
                    self.symbol_table.enter_scope();
                    // If condition is an optional type, the capture gets the inner type
                    if let Type::Option(inner_ty) = typed_condition.ty.clone() {
                        self.symbol_table.define(
                            cap.clone(),
                            *inner_ty,
                            Visibility::Private,
                            false,
                        );
                    }
                }

                let typed_then = self.infer_expr(then_branch)?;

                if capture.is_some() {
                    self.symbol_table.exit_scope();
                }

                let typed_else = self.infer_expr(else_branch)?;

                // The type of an if expression is the type of the then branch
                // (or the else branch if they differ, but for now we use then_branch type)
                let ty = typed_then.ty.clone();

                Ok(TypedExpr {
                    expr: TypedExprKind::If {
                        condition: Box::new(typed_condition),
                        capture: capture.clone(),
                        then_branch: Box::new(typed_then),
                        else_branch: Box::new(typed_else),
                    },
                    ty,
                    span: *span,
                })
            }
            Expr::Block { stmts, span } => {
                self.symbol_table.enter_scope();
                let mut typed_stmts = Vec::new();
                for s in stmts {
                    typed_stmts.push(self.infer_stmt(s)?);
                }
                self.symbol_table.exit_scope();

                // Try to get the type from the last expression statement
                let ty = typed_stmts
                    .last()
                    .and_then(|s| {
                        if let TypedStmtKind::Expr { expr } = &s.stmt {
                            Some(expr.ty.clone())
                        } else {
                            None
                        }
                    })
                    .unwrap_or(Type::Void);

                Ok(TypedExpr {
                    expr: TypedExprKind::Block { stmts: typed_stmts },
                    ty,
                    span: *span,
                })
            }
            Expr::MemberAccess {
                object,
                member,
                span,
                kind: _,
            } => {
                // Try to resolve as an enum/error variant first (Type.Variant)
                if let Expr::Ident(obj_name, _) = object.as_ref() {
                    let full_name = format!("{}.{}", obj_name, member);
                    let found_ty = self.symbol_table.resolve(&full_name).map(|s| s.ty.clone());
                    if let Some(ty) = found_ty {
                        // Determine if it's enum or error member
                        let kind = if self.enums.contains_key(obj_name) {
                            crate::ast::MemberAccessKind::EnumMember
                        } else if self.errors.contains_key(obj_name) {
                            crate::ast::MemberAccessKind::ErrorMember
                        } else {
                            crate::ast::MemberAccessKind::Unknown
                        };

                        return Ok(TypedExpr {
                            expr: TypedExprKind::MemberAccess {
                                object: Box::new(self.infer_expr(object)?),
                                member: member.clone(),
                                kind,
                            },
                            ty,
                            span: *span,
                        });
                    }
                }

                // Try to resolve as package access
                if let Expr::Ident(obj_name, _) = object.as_ref() {
                    // Check if obj_name is an imported package
                    let is_package = self.symbol_table.resolve(obj_name).is_none()
                        && (obj_name == "std" || obj_name == "io" || obj_name == "os"); // Simple check for now

                    if is_package {
                        return Ok(TypedExpr {
                            expr: TypedExprKind::MemberAccess {
                                object: Box::new(TypedExpr {
                                    expr: TypedExprKind::Ident(obj_name.clone()),
                                    ty: Type::I64, // Placeholder for package "type"
                                    span: Span { start: 0, end: 0 },
                                }),
                                member: member.clone(),
                                kind: crate::ast::MemberAccessKind::Package,
                            },
                            ty: Type::I64, // Placeholder
                            span: *span,
                        });
                    }
                }

                let typed_object = self.infer_expr(object)?;

                let (kind, ty) = if let Type::Custom { name, .. } = &typed_object.ty {
                    if let Some(struct_def) = self.structs.get(name) {
                        if let Some(method) = struct_def.methods.iter().find(|m| &m.name == member)
                        {
                            (
                                crate::ast::MemberAccessKind::StructMethod,
                                method.return_ty.clone(),
                            )
                        } else if let Some(field) =
                            struct_def.fields.iter().find(|f| &f.name == member)
                        {
                            (crate::ast::MemberAccessKind::StructField, field.ty.clone())
                        } else {
                            (crate::ast::MemberAccessKind::StructField, Type::I64)
                        }
                    } else if let Some(enum_def) = self.enums.get(name) {
                        if let Some(method) = enum_def.methods.iter().find(|m| &m.name == member) {
                            (
                                crate::ast::MemberAccessKind::StructMethod,
                                method.return_ty.clone(),
                            )
                        } else if enum_def.variants.iter().any(|v| &v.name == member) {
                            (
                                crate::ast::MemberAccessKind::EnumMember,
                                typed_object.ty.clone(),
                            )
                        } else {
                            (crate::ast::MemberAccessKind::EnumMember, Type::I64)
                        }
                    } else if let Some(error_def) = self.errors.get(name) {
                        if error_def.variants.iter().any(|v| &v.name == member) {
                            (crate::ast::MemberAccessKind::ErrorMember, Type::Error)
                        } else {
                            (crate::ast::MemberAccessKind::Unknown, Type::I64)
                        }
                    } else {
                        (crate::ast::MemberAccessKind::StructField, Type::I64)
                    }
                } else {
                    (crate::ast::MemberAccessKind::Unknown, Type::I64)
                };

                Ok(TypedExpr {
                    expr: TypedExprKind::MemberAccess {
                        object: Box::new(typed_object),
                        member: member.clone(),
                        kind,
                    },
                    ty,
                    span: *span,
                })
            }
            Expr::Struct { name, fields, span } => {
                let mut typed_fields = Vec::new();
                for (field_name, field_expr) in fields {
                    typed_fields.push((field_name.clone(), self.infer_expr(field_expr)?));
                }

                Ok(TypedExpr {
                    expr: TypedExprKind::Struct {
                        name: name.clone(),
                        fields: typed_fields,
                    },
                    ty: Type::Custom {
                        name: name.clone(),
                        generic_args: vec![],
                        is_exported: false,
                    },
                    span: *span,
                })
            }
            Expr::Try { expr, span } => {
                let typed_expr = self.infer_expr(expr)?;

                // Try unwraps the Result type to get the inner type
                let ty = if let Type::Result(inner) = &typed_expr.ty {
                    inner.as_ref().clone()
                } else {
                    typed_expr.ty.clone()
                };

                Ok(TypedExpr {
                    expr: TypedExprKind::Try {
                        expr: Box::new(typed_expr),
                    },
                    ty,
                    span: *span,
                })
            }
            Expr::Catch {
                expr,
                error_var,
                body,
                span,
            } => {
                let typed_expr = self.infer_expr(expr)?;

                // Handle error variable scope
                let has_error_var = error_var.is_some();
                if let Some(ev) = error_var {
                    self.symbol_table
                        .define(ev.clone(), Type::Error, Visibility::Private, false);
                }

                let typed_body = self.infer_expr(body)?;

                // Remove error variable from scope
                if has_error_var {
                    self.symbol_table.exit_scope();
                }

                // Catch expression returns the body type
                let body_ty = typed_body.ty.clone();
                Ok(TypedExpr {
                    expr: TypedExprKind::Catch {
                        expr: Box::new(typed_expr),
                        error_var: error_var.clone(),
                        body: Box::new(typed_body),
                    },
                    ty: body_ty,
                    span: *span,
                })
            }
        }
    }

    /// Infer the result type of a binary operation
    fn infer_binary_op_type(
        &self,
        op: &BinaryOp,
        left_ty: &Type,
        right_ty: &Type,
        span: &Span,
    ) -> AnalysisResult<Type> {
        match op {
            BinaryOp::Add
            | BinaryOp::Sub
            | BinaryOp::Mul
            | BinaryOp::Div
            | BinaryOp::Mod
            | BinaryOp::BitAnd
            | BinaryOp::BitOr
            | BinaryOp::BitXor
            | BinaryOp::Shl
            | BinaryOp::Shr => {
                // Numeric operations return the left operand type (or numeric)
                if self.is_numeric(left_ty) && self.is_numeric(right_ty) {
                    Ok(left_ty.clone())
                } else {
                    Err(AnalysisError::new_with_span(
                        "Binary operation requires numeric operands",
                        span,
                    )
                    .with_module("infer"))
                }
            }
            BinaryOp::Eq
            | BinaryOp::Ne
            | BinaryOp::Lt
            | BinaryOp::Gt
            | BinaryOp::Le
            | BinaryOp::Ge => {
                // Comparison operations return bool
                Ok(Type::Bool)
            }
            BinaryOp::And | BinaryOp::Or => {
                // Logical operations require bool operands
                if *left_ty == Type::Bool && *right_ty == Type::Bool {
                    Ok(Type::Bool)
                } else {
                    Err(AnalysisError::new_with_span(
                        "Logical operation requires boolean operands",
                        span,
                    )
                    .with_module("infer"))
                }
            }
            BinaryOp::Range => {
                // Range operator - return a tuple or iterator type
                Ok(Type::Tuple(vec![left_ty.clone(), right_ty.clone()]))
            }
        }
    }

    /// Infer the result type of a unary operation
    fn infer_unary_op_type(
        &self,
        op: &UnaryOp,
        expr_ty: &Type,
        span: &Span,
    ) -> AnalysisResult<Type> {
        match op {
            UnaryOp::Neg => {
                // Negation requires numeric type
                if self.is_numeric(expr_ty) {
                    Ok(expr_ty.clone())
                } else {
                    Err(
                        AnalysisError::new_with_span("Negation requires numeric operand", span)
                            .with_module("infer"),
                    )
                }
            }
            UnaryOp::Pos => {
                // Positive sign - just pass through
                Ok(expr_ty.clone())
            }
            UnaryOp::Not => {
                // Logical not requires bool
                if *expr_ty == Type::Bool {
                    Ok(Type::Bool)
                } else {
                    Err(
                        AnalysisError::new_with_span("Logical not requires boolean operand", span)
                            .with_module("infer"),
                    )
                }
            }
        }
    }

    /// Check if a type is numeric
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

    fn check_switch_exhaustiveness(
        &self,
        condition_ty: &Type,
        cases: &[TypedSwitchCase],
        span: &Span,
    ) -> AnalysisResult<()> {
        // If any case is a wildcard, it's exhaustive
        for case in cases {
            for pattern in &case.patterns {
                if let TypedExprKind::Ident(name) = &pattern.expr {
                    if name == "_" {
                        return Ok(());
                    }
                }
            }
        }

        match condition_ty {
            Type::Bool => {
                let mut has_true = false;
                let mut has_false = false;
                for case in cases {
                    for pattern in &case.patterns {
                        if let TypedExprKind::Bool(v) = &pattern.expr {
                            if *v {
                                has_true = true;
                            } else {
                                has_false = true;
                            }
                        }
                    }
                }
                if has_true && has_false {
                    Ok(())
                } else {
                    Err(AnalysisError::new_with_span(
                        "Switch statement on bool is not exhaustive",
                        span,
                    )
                    .with_module("infer"))
                }
            }
            Type::Custom { name, .. } => {
                if let Some(enum_def) = self.enums.get(name) {
                    let mut covered_variants = HashSet::new();
                    for case in cases {
                        for pattern in &case.patterns {
                            if let TypedExprKind::MemberAccess { member, .. } = &pattern.expr {
                                covered_variants.insert(member.clone());
                            }
                        }
                    }
                    if covered_variants.len() == enum_def.variants.len() {
                        Ok(())
                    } else {
                        let missing: Vec<_> = enum_def
                            .variants
                            .iter()
                            .filter(|v| !covered_variants.contains(&v.name))
                            .map(|v| v.name.clone())
                            .collect();
                        Err(AnalysisError::new_with_span(
                            format!(
                                "Switch statement on enum {} is not exhaustive. Missing: {}",
                                name,
                                missing.join(", ")
                            )
                            .as_str(),
                            span,
                        )
                        .with_module("infer"))
                    }
                } else {
                    Err(AnalysisError::new_with_span(format!("Switch statement on type {} requires a wildcard case '_' for exhaustiveness", condition_ty).as_str(), span).with_module("infer"))
                }
            }
            Type::Error => {
                let mut covered_variants = HashSet::new();
                for case in cases {
                    for pattern in &case.patterns {
                        if let TypedExprKind::MemberAccess { member, .. } = &pattern.expr {
                            covered_variants.insert(member.clone());
                        }
                    }
                }

                let total_variants: usize = self.errors.values().map(|e| e.variants.len()).sum();
                if covered_variants.len() == total_variants {
                    Ok(())
                } else {
                    Err(AnalysisError::new_with_span(
                        "Switch statement on error is not exhaustive",
                        span,
                    )
                    .with_module("infer"))
                }
            }
            _ => Err(AnalysisError::new_with_span(
                format!(
                    "Switch statement on type {} requires a wildcard case '_' for exhaustiveness",
                    condition_ty
                )
                .as_str(),
                span,
            )
            .with_module("infer")),
        }
    }
}

// ============================================================================
// Public API
// ============================================================================

/// Infer types for a program and produce a type-annotated AST
pub fn infer_types(program: &Program, symbol_table: SymbolTable) -> AnalysisResult<TypedProgram> {
    let mut inferrer = TypeInferrer::new(symbol_table);
    inferrer.infer_program(program)
}

// ============================================================================
// Pretty-Printing for Typed AST
// ============================================================================

pub use crate::ast::{AstDump, print_indent};

impl AstDump for TypedStructDef {
    fn dump(&self, indent: usize) {
        print_indent(indent);
        let vis = if self.visibility.is_public() {
            "pub "
        } else {
            ""
        };
        let generics = if self.generic_params.is_empty() {
            "".to_string()
        } else {
            format!("<{}>", self.generic_params.join(", "))
        };
        println!("StructDef: {}{}{}", vis, self.name, generics);

        for f in &self.fields {
            print_indent(indent + 1);
            let fvis = if f.visibility.is_public() { "pub " } else { "" };
            println!("Field: {}{}: {}", fvis, f.name, f.ty);
        }

        for m in &self.methods {
            m.dump(indent + 1);
        }
    }
}

impl AstDump for TypedEnumDef {
    fn dump(&self, indent: usize) {
        print_indent(indent);
        let vis = if self.visibility.is_public() {
            "pub "
        } else {
            ""
        };
        let generics = if self.generic_params.is_empty() {
            "".to_string()
        } else {
            format!("<{}>", self.generic_params.join(", "))
        };
        println!("EnumDef: {}{}{}", vis, self.name, generics);

        for v in &self.variants {
            print_indent(indent + 1);
            let vvis = if v.visibility.is_public() { "pub " } else { "" };
            let types = if v.associated_types.is_empty() {
                "".to_string()
            } else {
                format!("({:?})", v.associated_types)
            };
            println!("Variant: {}{}{}", vvis, v.name, types);
        }

        for m in &self.methods {
            m.dump(indent + 1);
        }
    }
}

impl AstDump for TypedErrorDef {
    fn dump(&self, indent: usize) {
        print_indent(indent);
        let vis = if self.visibility.is_public() {
            "pub "
        } else {
            ""
        };
        println!("ErrorDef: {}{}", vis, self.name);

        if let Some(union) = &self.union_types {
            print_indent(indent + 1);
            println!("Union: {:?}", union);
        } else {
            for v in &self.variants {
                print_indent(indent + 1);
                let vvis = if v.visibility.is_public() { "pub " } else { "" };
                let types = if v.associated_types.is_empty() {
                    "".to_string()
                } else {
                    format!("({:?})", v.associated_types)
                };
                println!("Variant: {}{}{}", vvis, v.name, types);
            }
        }
    }
}

impl AstDump for TypedProgram {
    fn dump(&self, indent: usize) {
        print_indent(indent);
        println!("TypedProgram");

        if !self.imports.is_empty() {
            print_indent(indent + 1);
            println!("Imports:");
            for (alias, pkg) in &self.imports {
                print_indent(indent + 2);
                if let Some(a) = alias {
                    println!("{}: \"{}\"", a, pkg);
                } else {
                    println!("\"{}\"", pkg);
                }
            }
        }

        for s in &self.structs {
            s.dump(indent + 1);
        }
        for e in &self.enums {
            e.dump(indent + 1);
        }
        for er in &self.errors {
            er.dump(indent + 1);
        }
        for f in &self.external_functions {
            f.dump(indent + 1);
        }
        for f in &self.functions {
            f.dump(indent + 1);
        }
    }
}

impl AstDump for TypedFnDef {
    fn dump(&self, indent: usize) {
        print_indent(indent);
        let vis = if self.visibility.is_public() {
            "pub "
        } else {
            ""
        };
        println!("FnDef: {}{} -> {}", vis, self.name, self.return_ty);

        if !self.params.is_empty() {
            print_indent(indent + 1);
            println!("Params:");
            for p in &self.params {
                print_indent(indent + 2);
                println!("{}: {}", p.name, p.ty);
            }
        }

        print_indent(indent + 1);
        println!("Body:");
        for s in &self.body {
            s.dump(indent + 2);
        }
    }
}

impl AstDump for TypedStmt {
    fn dump(&self, indent: usize) {
        print_indent(indent);
        match &self.stmt {
            TypedStmtKind::Expr { expr } => {
                println!("Stmt::Expr (ty: {})", expr.ty);
                expr.dump(indent + 1);
            }
            TypedStmtKind::Import { packages } => {
                println!("Stmt::Import: {:?}", packages);
            }
            TypedStmtKind::Let {
                name,
                names,
                ty,
                value,
                is_const,
            } => {
                let mut_str = if *is_const { "const" } else { "var" };
                let name_str = if let Some(ns) = names {
                    format!("{:?}", ns)
                } else {
                    name.clone()
                };
                println!("Stmt::Let: {} {} (ty: {})", mut_str, name_str, ty);
                if let Some(v) = value {
                    print_indent(indent + 1);
                    println!("Value:");
                    v.dump(indent + 2);
                }
            }
            TypedStmtKind::Assign { target, value } => {
                println!("Stmt::Assign: {} (ty: {})", target, value.ty);
                print_indent(indent + 1);
                println!("Value:");
                value.dump(indent + 2);
            }
            TypedStmtKind::Return { value } => {
                println!("Stmt::Return");
                if let Some(v) = value {
                    print_indent(indent + 1);
                    println!("Value (ty: {}):", v.ty);
                    v.dump(indent + 2);
                }
            }
            TypedStmtKind::Block { stmts } => {
                println!("Stmt::Block");
                for s in stmts {
                    s.dump(indent + 1);
                }
            }
            TypedStmtKind::If {
                condition,
                capture,
                then_branch,
                else_branch,
            } => {
                println!("Stmt::If");
                if let Some(c) = capture {
                    let cap_ty = if let Type::Option(inner) = &condition.ty {
                        inner.as_ref().clone()
                    } else {
                        Type::I64
                    };
                    print_indent(indent + 1);
                    println!("Capture: {} (ty: {})", c, cap_ty);
                }
                print_indent(indent + 1);
                println!("Condition (ty: {}):", condition.ty);
                condition.dump(indent + 2);
                print_indent(indent + 1);
                println!("Then:");
                then_branch.dump(indent + 2);
                if let Some(eb) = else_branch {
                    print_indent(indent + 1);
                    println!("Else:");
                    eb.dump(indent + 2);
                }
            }
            TypedStmtKind::While {
                condition,
                capture,
                body,
            } => {
                println!("Stmt::While");
                if let Some(c) = capture {
                    let cap_ty = if let Type::Option(inner) = &condition.ty {
                        inner.as_ref().clone()
                    } else {
                        Type::I64
                    };
                    print_indent(indent + 1);
                    println!("Capture: {} (ty: {})", c, cap_ty);
                }
                print_indent(indent + 1);
                println!("Condition (ty: {}):", condition.ty);
                condition.dump(indent + 2);
                print_indent(indent + 1);
                println!("Body:");
                body.dump(indent + 2);
            }
            TypedStmtKind::For {
                label,
                var_name,
                iterable,
                capture,
                index_var,
                body,
            } => {
                println!("Stmt::For");
                if let Some(l) = label {
                    print_indent(indent + 1);
                    println!("Label: {}", l);
                }
                if let Some(v) = var_name {
                    print_indent(indent + 1);
                    println!("Var: {} (ty: i64)", v);
                }
                if let Some(i) = index_var {
                    print_indent(indent + 1);
                    println!("Index: {} (ty: i64)", i);
                }
                if let Some(c) = capture {
                    print_indent(indent + 1);
                    println!("Capture: {} (ty: i64)", c);
                }
                print_indent(indent + 1);
                println!("Iterable (ty: {}):", iterable.ty);
                iterable.dump(indent + 2);
                print_indent(indent + 1);
                println!("Body:");
                body.dump(indent + 2);
            }
            TypedStmtKind::Switch { condition, cases } => {
                println!("Stmt::Switch");
                print_indent(indent + 1);
                println!("Condition (ty: {}):", condition.ty);
                condition.dump(indent + 2);
                for (i, case) in cases.iter().enumerate() {
                    print_indent(indent + 1);
                    println!("Case {}:", i);
                    if let Some(c) = &case.capture {
                        print_indent(indent + 2);
                        println!("Capture: {} (ty: {})", c, condition.ty);
                    }
                    print_indent(indent + 2);
                    println!("Patterns:");
                    for p in &case.patterns {
                        p.dump(indent + 3);
                    }
                    print_indent(indent + 2);
                    println!("Body:");
                    case.body.dump(indent + 3);
                }
            }
            TypedStmtKind::Defer { stmt } => {
                println!("Stmt::Defer");
                stmt.dump(indent + 1);
            }
            TypedStmtKind::DeferBang { stmt } => {
                println!("Stmt::Defer!");
                stmt.dump(indent + 1);
            }
            TypedStmtKind::Break { label } => {
                let lbl = if let Some(l) = label {
                    format!(" {}", l)
                } else {
                    "".to_string()
                };
                println!("Stmt::Break{}", lbl);
            }
        }
    }
}

impl AstDump for TypedExpr {
    fn dump(&self, indent: usize) {
        print_indent(indent);
        match &self.expr {
            TypedExprKind::Int(val) => println!("Expr::Int({}) (ty: {})", val, self.ty),
            TypedExprKind::Float(val) => println!("Expr::Float({}) (ty: {})", val, self.ty),
            TypedExprKind::Bool(val) => println!("Expr::Bool({}) (ty: {})", val, self.ty),
            TypedExprKind::String(val) => println!("Expr::String(\"{}\") (ty: {})", val, self.ty),
            TypedExprKind::Char(val) => println!("Expr::Char('{}') (ty: {})", val, self.ty),
            TypedExprKind::Null => println!("Expr::Null (ty: {})", self.ty),
            TypedExprKind::Tuple(exprs) => {
                println!("Expr::Tuple (ty: {})", self.ty);
                for e in exprs {
                    e.dump(indent + 1);
                }
            }
            TypedExprKind::TupleIndex { tuple, index } => {
                println!("Expr::TupleIndex: .{} (ty: {})", index, self.ty);
                tuple.dump(indent + 1);
            }
            TypedExprKind::Ident(name) => println!("Expr::Ident({}) (ty: {})", name, self.ty),
            TypedExprKind::Array(exprs) => {
                println!("Expr::Array (ty: {})", self.ty);
                for e in exprs {
                    e.dump(indent + 1);
                }
            }
            TypedExprKind::Binary { op, left, right } => {
                println!("Expr::Binary: {:?} (ty: {})", op, self.ty);
                left.dump(indent + 1);
                right.dump(indent + 1);
            }
            TypedExprKind::Unary { op, expr } => {
                println!("Expr::Unary: {:?} (ty: {})", op, self.ty);
                expr.dump(indent + 1);
            }
            TypedExprKind::Call {
                name,
                namespace,
                args,
            } => {
                let ns = if let Some(n) = namespace {
                    format!("{}::", n)
                } else {
                    "".to_string()
                };
                println!("Expr::Call: {}{} (ty: {})", ns, name, self.ty);
                for a in args {
                    a.dump(indent + 1);
                }
            }
            TypedExprKind::If {
                condition,
                capture,
                then_branch,
                else_branch,
            } => {
                println!("Expr::If (ty: {})", self.ty);
                if let Some(c) = capture {
                    let cap_ty = if let Type::Option(inner) = &condition.ty {
                        inner.as_ref().clone()
                    } else {
                        Type::I64
                    };
                    print_indent(indent + 1);
                    println!("Capture: {} (ty: {})", c, cap_ty);
                }
                print_indent(indent + 1);
                println!("Condition (ty: {}):", condition.ty);
                condition.dump(indent + 2);
                print_indent(indent + 1);
                println!("Then (ty: {}):", then_branch.ty);
                then_branch.dump(indent + 2);
                print_indent(indent + 1);
                println!("Else (ty: {}):", else_branch.ty);
                else_branch.dump(indent + 2);
            }
            TypedExprKind::Block { stmts } => {
                println!("Expr::Block (ty: {})", self.ty);
                for s in stmts {
                    s.dump(indent + 1);
                }
            }
            TypedExprKind::MemberAccess {
                object,
                member,
                kind,
            } => {
                let kind_str = match kind {
                    crate::ast::MemberAccessKind::Unknown => "",
                    crate::ast::MemberAccessKind::Package => " (package)",
                    crate::ast::MemberAccessKind::StructField => " (field)",
                    crate::ast::MemberAccessKind::StructMethod => " (method)",
                    crate::ast::MemberAccessKind::EnumMember => " (enum)",
                    crate::ast::MemberAccessKind::ErrorMember => " (error)",
                };
                println!(
                    "Expr::MemberAccess: .{}{} (ty: {})",
                    member, kind_str, self.ty
                );
                object.dump(indent + 1);
            }
            TypedExprKind::Struct { name, fields } => {
                println!("Expr::Struct: {} (ty: {})", name, self.ty);
                for (fname, fval) in fields {
                    print_indent(indent + 1);
                    println!("Field: {}:", fname);
                    fval.dump(indent + 2);
                }
            }
            TypedExprKind::Try { expr } => {
                println!("Expr::Try (ty: {})", self.ty);
                expr.dump(indent + 1);
            }
            TypedExprKind::Catch {
                expr,
                error_var,
                body,
            } => {
                println!("Expr::Catch (ty: {})", self.ty);
                if let Some(v) = error_var {
                    print_indent(indent + 1);
                    println!("Capture: {} (ty: Error)", v);
                }
                print_indent(indent + 1);
                println!("Value (ty: {}):", expr.ty);
                expr.dump(indent + 2);
                print_indent(indent + 1);
                println!("Body (ty: {}):", body.ty);
                body.dump(indent + 2);
            }
        }
    }
}
