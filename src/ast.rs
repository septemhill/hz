//! # Abstract Syntax Tree (AST) for Lang Programming Language
//!
//! This module defines all AST nodes that represent the parsed program structure.

use std::collections::HashMap;
use std::fmt;

/// Represents a data type in the language
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
        args: Vec<Expr>,
        span: Span,
    },
}

/// Statement AST node
#[derive(Debug, Clone)]
pub enum Stmt {
    /// Expression statement (with semicolon)
    Expr { expr: Expr, span: Span },
    /// Variable declaration (let)
    Let {
        mutable: bool,
        name: String,
        ty: Option<Type>,
        value: Option<Expr>,
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
