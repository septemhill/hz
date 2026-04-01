use crate::ast::{AssignOp, BinaryOp, Mutability, Span, Type, UnaryOp, Visibility};
use std::fmt;

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
    Index {
        object: Box<HirExpr>,
        index: Box<HirExpr>,
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
        /// Target type for method calls - used to resolve method name to monomorphized version
        target_ty: Option<Type>,
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
    /// Pointer dereference
    Dereference {
        expr: Box<HirExpr>,
        ty: Type,
        span: Span,
    },
    /// Intrinsic function operator (e.g., @is_null)
    Intrinsic {
        name: String,
        args: Vec<HirExpr>,
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
            HirExpr::Index { ty, .. } => ty,
            HirExpr::Array { ty, .. } => ty,
            HirExpr::Binary { ty, .. } => ty,
            HirExpr::Unary { ty, .. } => ty,
            HirExpr::Call { return_ty, .. } => return_ty,
            HirExpr::Intrinsic { ty, .. } => ty,
            HirExpr::If { ty, .. } => ty,
            HirExpr::Block { ty, .. } => ty,
            HirExpr::MemberAccess { ty, .. } => ty,
            HirExpr::Struct { ty, .. } => ty,
            HirExpr::Try { ty, .. } => ty,
            HirExpr::Catch { ty, .. } => ty,
            HirExpr::Cast { ty, .. } => ty,
            HirExpr::Dereference { ty, .. } => ty,
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
        op: AssignOp,
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
    /// Continue statement (skips to next iteration)
    Continue {
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

impl fmt::Display for HirProgram {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, func) in self.functions.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            write!(f, "{}", func)?;
        }
        Ok(())
    }
}

impl fmt::Display for HirFn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let vis = match self.visibility {
            Visibility::Public => "pub ",
            Visibility::Private => "",
        };
        write!(f, "{}fn {}(", vis, self.name)?;
        for (i, (param_name, param_ty)) in self.params.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}: {}", param_name, param_ty)?;
        }
        write!(f, ") -> {}", self.return_ty)?;
        writeln!(f, " {{")?;
        for stmt in &self.body {
            write!(f, "{}", stmt.with_indent(1))?;
        }
        write!(f, "}}")?;
        Ok(())
    }
}

impl HirStmt {
    pub fn with_indent(&self, indent: usize) -> String {
        let indent_str = "    ".repeat(indent);
        match self {
            HirStmt::Expr(expr) => format!("{}{};\n", indent_str, expr),
            HirStmt::Let {
                name,
                ty,
                value,
                mutability,
                ..
            } => {
                let mut_str = match mutability {
                    Mutability::Var => "var ",
                    Mutability::Const => "",
                };
                match value {
                    Some(v) => format!("{}{}let {} {} = {};\n", indent_str, mut_str, name, ty, v),
                    None => format!("{}{}let {}: {};\n", indent_str, mut_str, name, ty),
                }
            }
            HirStmt::Assign {
                target, op, value, ..
            } => {
                let op_str = match op {
                    AssignOp::Assign => "=",
                    AssignOp::AddAssign => "+=",
                    AssignOp::SubAssign => "-=",
                    AssignOp::MulAssign => "*=",
                    AssignOp::DivAssign => "/=",
                    AssignOp::ModAssign => "%=",
                    AssignOp::AndAssign => "&=",
                    AssignOp::OrAssign => "|=",
                    AssignOp::XorAssign => "^=",
                    AssignOp::ShlAssign => "<<=",
                    AssignOp::ShrAssign => ">>=",
                };
                format!("{}{} {} {};\n", indent_str, target, op_str, value)
            }
            HirStmt::Return(expr, _) => match expr {
                Some(e) => format!("{}return {};\n", indent_str, e),
                None => format!("{}return;\n", indent_str),
            },
            HirStmt::If {
                condition,
                capture,
                then_branch,
                else_branch,
                ..
            } => {
                let capture_str = match capture {
                    Some(c) => format!(" @{}", c),
                    None => String::new(),
                };
                let mut result = format!("{}{}if{} {{", indent_str, condition, capture_str);
                result.push_str(&then_branch.with_indent(indent + 1));
                result.push_str(&format!("{}}}", indent_str));
                if let Some(else_br) = else_branch {
                    result.push_str(" else {");
                    result.push_str(&else_br.with_indent(indent + 1));
                    result.push_str(&format!("{}}}", indent_str));
                }
                result.push('\n');
                result
            }
            HirStmt::Switch {
                condition, cases, ..
            } => {
                let mut result = format!("{}switch {} {{\n", indent_str, condition);
                for case in cases {
                    result.push_str(&case.with_indent(indent + 1));
                }
                result.push_str(&format!("{}}}\n", indent_str));
                result
            }
            HirStmt::For {
                label,
                var_name,
                index_var,
                iterable,
                body,
                ..
            } => {
                let label_str = match label {
                    Some(l) => format!("'{}", l),
                    None => String::new(),
                };
                let var_str = match var_name {
                    Some(v) => format!(", {}", v),
                    None => String::new(),
                };
                let idx_str = match index_var {
                    Some(i) => format!(", {}", i),
                    None => String::new(),
                };
                let mut result = format!(
                    "{}for{}{}{} in {} {{\n",
                    indent_str, label_str, var_str, idx_str, iterable
                );
                result.push_str(&body.with_indent(indent + 1));
                result.push_str(&format!("{}}}\n", indent_str));
                result
            }
            HirStmt::Defer { stmt, .. } => {
                let mut result = format!("{}defer {{", indent_str);
                result.push_str(&stmt.with_indent(indent + 1));
                result.push_str(&format!("{}}}\n", indent_str));
                result
            }
            HirStmt::DeferBang { stmt, .. } => {
                let mut result = format!("{}defer! {{", indent_str);
                result.push_str(&stmt.with_indent(indent + 1));
                result.push_str(&format!("{}}}\n", indent_str));
                result
            }
            HirStmt::Break { label, .. } => match label {
                Some(l) => format!("{}break '{};\n", indent_str, l),
                None => format!("{}break;\n", indent_str),
            },
            HirStmt::Continue { label, .. } => match label {
                Some(l) => format!("{}continue '{};\n", indent_str, l),
                None => format!("{}continue;\n", indent_str),
            },
        }
    }
}

impl fmt::Display for HirStmt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.with_indent(0))
    }
}

impl HirCase {
    pub fn with_indent(&self, indent: usize) -> String {
        let indent_str = "    ".repeat(indent);
        let patterns = self
            .patterns
            .iter()
            .map(|p| p.to_string())
            .collect::<Vec<_>>()
            .join(" | ");
        let mut result = format!("{}{} => {{", indent_str, patterns);
        result.push_str(&self.body.with_indent(indent + 1));
        result.push_str(&format!("{}}}", indent_str));
        result.push('\n');
        result
    }
}

impl fmt::Display for HirCase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.with_indent(0))
    }
}

impl fmt::Display for HirExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HirExpr::Int(n, ty, _) => write!(f, "{}: {}", n, ty),
            HirExpr::Float(n, ty, _) => write!(f, "{}: {}", n, ty),
            HirExpr::Bool(b, ty, _) => write!(f, "{}: {}", b, ty),
            HirExpr::String(s, ty, _) => write!(f, "\"{}\": {}", s, ty),
            HirExpr::Char(c, ty, _) => write!(f, "'{}': {}", c, ty),
            HirExpr::Null(ty, _) => write!(f, "null: {}", ty),
            HirExpr::Ident(name, ty, _) => write!(f, "{}: {}", name, ty),
            HirExpr::Tuple { vals, ty, .. } => {
                let vals_str = vals
                    .iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "({}): {}", vals_str, ty)
            }
            HirExpr::TupleIndex {
                tuple, index, ty, ..
            } => {
                write!(f, "{}.{}: {}", tuple, index, ty)
            }
            HirExpr::Index {
                object, index, ty, ..
            } => {
                write!(f, "{}[{}]: {}", object, index, ty)
            }
            HirExpr::Array { vals, ty, .. } => {
                let vals_str = vals
                    .iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "[{}]: {}", vals_str, ty)
            }
            HirExpr::Binary {
                op,
                left,
                right,
                ty,
                ..
            } => {
                write!(f, "({} {} {}): {}", left, op, right, ty)
            }
            HirExpr::Unary { op, expr, ty, .. } => {
                write!(f, "({}{}): {}", op, expr, ty)
            }
            HirExpr::Call {
                name,
                namespace,
                args,
                return_ty,
                target_ty,
                ..
            } => {
                let ns = namespace
                    .as_ref()
                    .map(|n| format!("{}::", n))
                    .unwrap_or_default();
                let target = target_ty
                    .as_ref()
                    .map(|t| format!("[{}]", t))
                    .unwrap_or_default();
                let args_str = args
                    .iter()
                    .map(|a| a.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(
                    f,
                    "{}({}){}: {}",
                    format!("{}{}", ns, name),
                    args_str,
                    target,
                    return_ty
                )
            }
            HirExpr::If {
                condition,
                capture,
                then_branch,
                else_branch,
                ty,
                ..
            } => {
                let capture_str = match capture {
                    Some(c) => format!(" @{}", c),
                    None => String::new(),
                };
                write!(
                    f,
                    "if{} {} then {} else {}",
                    capture_str, condition, then_branch, else_branch
                )
            }
            HirExpr::Block {
                stmts, expr, ty, ..
            } => {
                write!(f, "{{ ")?;
                for stmt in stmts {
                    write!(f, "{}; ", stmt)?;
                }
                if let Some(e) = expr {
                    write!(f, "{}", e)?;
                }
                write!(f, " }}: {}", ty)
            }
            HirExpr::MemberAccess {
                object, member, ty, ..
            } => {
                write!(f, "{}.{}: {}", object, member, ty)
            }
            HirExpr::Struct {
                name, fields, ty, ..
            } => {
                let fields_str = fields
                    .iter()
                    .map(|(n, v)| format!("{}: {}", n, v))
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "{} {{ {} }}: {}", name, fields_str, ty)
            }
            HirExpr::Try { expr, ty, .. } => {
                write!(f, "try {}", expr)
            }
            HirExpr::Catch {
                expr,
                error_var,
                body,
                ty,
                ..
            } => {
                let err_str = error_var
                    .as_ref()
                    .map(|e| format!(" @{}", e))
                    .unwrap_or_default();
                write!(f, "catch{} {} {}", err_str, expr, body)
            }
            HirExpr::Cast {
                target_type,
                expr,
                ty,
                ..
            } => {
                write!(f, "({} as {}): {}", expr, target_type, ty)
            }
            HirExpr::Dereference { expr, ty, .. } => {
                write!(f, "(*{}): {}", expr, ty)
            }
            HirExpr::Intrinsic { name, args, ty, .. } => {
                let args_str = args
                    .iter()
                    .map(|a| a.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "{}({}): {}", name, args_str, ty)
            }
        }
    }
}
