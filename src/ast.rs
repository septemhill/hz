//! # Abstract Syntax Tree (AST) for Lang Programming Language
//!
//! This module defines all AST nodes that represent the parsed program structure.

use std::collections::HashMap;
use std::fmt;

/// Represents a data type in the language
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    Bool,
    Void,
    /// Optional type (e.g., ?i32)
    Option(Box<Type>),
    /// Custom type (struct or enum)
    Custom {
        name: String,
        /// Generic type arguments
        generic_args: Vec<Type>,
        /// Whether this is an external/exported type
        is_exported: bool,
    },
    /// Generic type parameter (for generics)
    GenericParam(String),
}

impl Type {
    /// Get the default type for literals
    pub fn default_for_literal(literal: &str) -> Type {
        if literal.parse::<i64>().is_ok() {
            Type::I64
        } else if literal.parse::<u64>().is_ok() {
            Type::U64
        } else {
            Type::I64
        }
    }

    /// Check if this is a custom type
    pub fn is_custom(&self) -> bool {
        matches!(self, Type::Custom { .. })
    }

    /// Get the name of a custom type
    pub fn custom_name(&self) -> Option<&String> {
        match self {
            Type::Custom { name, .. } => Some(name),
            _ => None,
        }
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::I8 => write!(f, "i8"),
            Type::I16 => write!(f, "i16"),
            Type::I32 => write!(f, "i32"),
            Type::I64 => write!(f, "i64"),
            Type::U8 => write!(f, "u8"),
            Type::U16 => write!(f, "u16"),
            Type::U32 => write!(f, "u32"),
            Type::U64 => write!(f, "u64"),
            Type::Bool => write!(f, "bool"),
            Type::Void => write!(f, "void"),
            Type::Option(inner) => write!(f, "?{}", inner),
            Type::Custom {
                name, generic_args, ..
            } => {
                if generic_args.is_empty() {
                    write!(f, "{}", name)
                } else {
                    let args: Vec<String> = generic_args.iter().map(|a| a.to_string()).collect();
                    write!(f, "{}<{}>", name, args.join(", "))
                }
            }
            Type::GenericParam(name) => write!(f, "{}", name),
        }
    }
}

/// Binary operators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    And,
    Or,
}

impl BinaryOp {
    /// Get the precedence of the binary operator (higher = binds tighter)
    pub fn precedence(self) -> u8 {
        match self {
            BinaryOp::Or => 1,
            BinaryOp::And => 2,
            BinaryOp::Eq | BinaryOp::Ne => 3,
            BinaryOp::Lt | BinaryOp::Gt | BinaryOp::Le | BinaryOp::Ge => 4,
            BinaryOp::Add | BinaryOp::Sub => 5,
            BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => 6,
        }
    }
}

/// Unary operators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg, // - (negation)
    Pos, // + (positive)
    Not, // ! (logical not)
}

/// Assignment operators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    Assign,
    AddAssign,
    SubAssign,
    MulAssign,
    DivAssign,
}

/// Visibility modifier (pub keyword)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Private,
    Public,
}

impl Visibility {
    pub fn is_public(&self) -> bool {
        matches!(self, Visibility::Public)
    }
}

/// Variable mutability (var or const)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mutability {
    Var,   // mutable variable
    Const, // immutable constant
}

/// Struct field definition
#[derive(Debug, Clone)]
pub struct StructField {
    pub name: String,
    pub ty: Type,
    pub visibility: Visibility,
}

/// Struct definition
#[derive(Debug, Clone)]
pub struct StructDef {
    pub name: String,
    pub fields: Vec<StructField>,
    pub methods: Vec<FnDef>,
    pub visibility: Visibility,
    /// Generic type parameters (e.g., T, U)
    pub generic_params: Vec<String>,
    pub span: Span,
}

/// Enum variant
#[derive(Debug, Clone)]
pub struct EnumVariant {
    pub name: String,
    pub associated_types: Vec<Type>,
    pub visibility: Visibility,
}

/// Enum definition
#[derive(Debug, Clone)]
pub struct EnumDef {
    pub name: String,
    pub variants: Vec<EnumVariant>,
    pub methods: Vec<FnDef>,
    pub visibility: Visibility,
    /// Generic type parameters (e.g., T, U)
    pub generic_params: Vec<String>,
    pub span: Span,
}

/// Position in source code (line, column)
#[derive(Debug, Clone, Copy, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

/// Expression AST node
#[derive(Debug, Clone)]
pub enum Expr {
    /// Integer literal (i64)
    Int(i64, Span),
    /// Boolean literal
    Bool(bool, Span),
    /// String literal
    String(String, Span),
    /// Null literal
    Null(Span),
    /// Variable identifier
    Ident(String, Span),
    /// Binary operation
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
        span: Span,
    },
    /// Unary operation
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
        span: Span,
    },
    /// Function call
    Call {
        name: String,
        namespace: Option<String>,
        args: Vec<Expr>,
        span: Span,
    },
}

/// Statement AST node
#[derive(Debug, Clone)]
pub enum Stmt {
    /// Expression statement (with semicolon)
    Expr { expr: Expr, span: Span },
    /// Import statement with optional alias
    /// Syntax: import "package" or import alias "package"
    Import {
        packages: Vec<(Option<String>, String)>,
        span: Span,
    },
    /// Variable declaration (var or const)
    Let {
        mutability: Mutability,
        name: String,
        ty: Option<Type>,
        value: Option<Expr>,
        visibility: Visibility,
        span: Span,
    },
    /// Assignment statement
    Assign {
        target: String,
        op: AssignOp,
        value: Expr,
        span: Span,
    },
    /// Return statement
    Return { value: Option<Expr>, span: Span },
    /// Block statement (sequence of statements)
    Block { stmts: Vec<Stmt>, span: Span },
    /// If statement
    If {
        condition: Expr,
        then_branch: Box<Stmt>,
        else_branch: Option<Box<Stmt>>,
        span: Span,
    },
    /// While loop
    While {
        condition: Expr,
        body: Box<Stmt>,
        span: Span,
    },
    /// Infinite loop
    Loop { body: Box<Stmt>, span: Span },
}

/// Function definition AST node
#[derive(Debug, Clone)]
pub struct FnDef {
    pub name: String,
    pub visibility: Visibility,
    pub params: Vec<FnParam>,
    pub return_ty: Option<Type>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

/// Function parameter
#[derive(Debug, Clone)]
pub struct FnParam {
    pub name: String,
    pub ty: Type,
}

/// Program AST node (root of the tree)
#[derive(Debug, Clone)]
pub struct Program {
    pub functions: Vec<FnDef>,
    pub structs: Vec<StructDef>,
    pub enums: Vec<EnumDef>,
    pub imports: Vec<(Option<String>, String)>, // (alias, package_name)
}

/// Visitor trait for AST traversal
pub trait ASTVisitor<T> {
    fn visit_expr(&mut self, expr: &Expr) -> T;
    fn visit_stmt(&mut self, stmt: &Stmt) -> T;
    fn visit_program(&mut self, program: &Program) -> T;
}

/// Helper to create spans (placeholder implementation)
pub fn span(start: usize, end: usize) -> Span {
    Span { start, end }
}
