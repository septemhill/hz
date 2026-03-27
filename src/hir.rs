use crate::ast::{BinaryOp, Mutability, Span, Type, UnaryOp, Visibility};

#[derive(Debug, Clone)]
#[allow(unused)]
pub enum HirExpr {
    Int(i64, Type, Span),
    Float(f64, Type, Span),
    Bool(bool, Type, Span),
    String(String, Type, Span),
    Char(char, Type, Span),
    Null(Type, Span),
    Ident(String, Type, Span),
    Tuple {
        vals: Vec<HirExpr>,
        ty: Type,
        span: Span,
    },
    TupleIndex {
        tuple: Box<HirExpr>,
        index: usize,
        ty: Type,
        span: Span,
    },
    Array {
        vals: Vec<HirExpr>,
        ty: Type,
        span: Span,
    },
    Binary {
        op: BinaryOp,
        left: Box<HirExpr>,
        right: Box<HirExpr>,
        ty: Type,
        span: Span,
    },
    Unary {
        op: UnaryOp,
        expr: Box<HirExpr>,
        ty: Type,
        span: Span,
    },
    Call {
        name: String,
        namespace: Option<String>,
        args: Vec<HirExpr>,
        return_ty: Type,
        span: Span,
    },
    // Desugared constructs
    If {
        condition: Box<HirExpr>,
        capture: Option<String>,
        then_branch: Box<HirExpr>,
        else_branch: Box<HirExpr>,
        ty: Type,
        span: Span,
    },
    Block {
        stmts: Vec<HirStmt>,
        expr: Option<Box<HirExpr>>,
        ty: Type,
        span: Span,
    },
    MemberAccess {
        object: Box<HirExpr>,
        member: String,
        ty: Type,
        span: Span,
    },
    Struct {
        name: String,
        fields: Vec<(String, HirExpr)>,
        ty: Type,
        span: Span,
    },
    Try {
        expr: Box<HirExpr>,
        ty: Type,
        span: Span,
    },
    /// Catch expression
    Catch {
        expr: Box<HirExpr>,
        error_var: Option<String>,
        body: Box<HirExpr>,
        ty: Type,
        span: Span,
    },
    /// Type cast expression
    Cast {
        target_type: Type,
        expr: Box<HirExpr>,
        ty: Type,
        span: Span,
    },
}

impl HirExpr {
    pub fn ty(&self) -> &Type {
        match self {
            HirExpr::Int(_, ty, _) => ty,
            HirExpr::Float(_, ty, _) => ty,
            HirExpr::Bool(_, ty, _) => ty,
            HirExpr::String(_, ty, _) => ty,
            HirExpr::Char(_, ty, _) => ty,
            HirExpr::Null(ty, _) => ty,
            HirExpr::Ident(_, ty, _) => ty,
            HirExpr::Tuple { ty, .. } => ty,
            HirExpr::TupleIndex { ty, .. } => ty,
            HirExpr::Array { ty, .. } => ty,
            HirExpr::Binary { ty, .. } => ty,
            HirExpr::Unary { ty, .. } => ty,
            HirExpr::Call { return_ty, .. } => return_ty,
            HirExpr::If { ty, .. } => ty,
            HirExpr::Block { ty, .. } => ty,
            HirExpr::MemberAccess { ty, .. } => ty,
            HirExpr::Struct { ty, .. } => ty,
            HirExpr::Try { ty, .. } => ty,
            HirExpr::Catch { ty, .. } => ty,
            HirExpr::Cast { ty, .. } => ty,
        }
    }
}

#[derive(Debug, Clone)]
#[allow(unused)]
pub enum HirStmt {
    Expr(HirExpr),
    Let {
        name: String,
        ty: Type,
        value: Option<HirExpr>,
        mutability: Mutability,
        span: Span,
    },
    Assign {
        target: String,
        value: HirExpr,
        span: Span,
    },
    Return(Option<HirExpr>, Span),
    If {
        condition: HirExpr,
        capture: Option<String>,
        then_branch: Box<HirStmt>,
        else_branch: Option<Box<HirStmt>>,
        span: Span,
    },
    // Switch will be desugared into nested If/Else or a Jump Table HIR node
    Switch {
        condition: HirExpr,
        cases: Vec<HirCase>,
        span: Span,
    },
    /// For loop
    For {
        label: Option<String>,
        var_name: Option<String>,
        index_var: Option<String>,
        iterable: HirExpr,
        body: Box<HirStmt>,
        span: Span,
    },
    /// Defer statement (executes on scope exit)
    Defer {
        stmt: Box<HirStmt>,
        span: Span,
    },
    /// DeferBang statement (executes only on error in try statement)
    DeferBang {
        stmt: Box<HirStmt>,
        span: Span,
    },
    /// Break statement (exits a loop)
    Break {
        label: Option<String>,
        span: Span,
    },
}

#[derive(Debug, Clone)]
#[allow(unused)]
pub struct HirCase {
    pub patterns: Vec<HirExpr>,
    pub body: HirStmt,
    pub span: Span,
}

#[derive(Debug, Clone)]
#[allow(unused)]
pub struct HirFn {
    pub name: String,
    pub params: Vec<(String, Type)>,
    pub return_ty: Type,
    pub body: Vec<HirStmt>,
    pub visibility: Visibility,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct HirProgram {
    pub functions: Vec<HirFn>,
    // Add structs, enums, etc.
}
