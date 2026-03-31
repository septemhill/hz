//! # Abstract Syntax Tree (AST) for Lang Programming Language
//!
//! This module defines all AST nodes that represent the parsed program structure.

use std::fmt;

/// Represents a data type in the language
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(dead_code)]
pub enum Type {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    F32,
    F64,
    /// Immediate integer value (unresolved literal)
    ImmInt,
    /// Immediate float value (unresolved literal)
    ImmFloat,
    Bool,
    Void,
    /// Raw pointer type (opaque pointer, similar to C's void*)
    /// Cannot perform arithmetic operations directly, must convert to u64
    RawPtr,
    /// Self type (for struct methods)
    SelfType,
    /// Pointer type (e.g., *i32)
    Pointer(Box<Type>),
    /// Optional type (e.g., ?i32)
    Option(Box<Type>),
    /// Tuple type (e.g., (i32, i64))
    Tuple(Vec<Type>),
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
    /// Array type (e.g., [3]u8)
    Array {
        /// Size of the array (if known at compile time)
        size: Option<usize>,
        /// Element type
        element_type: Box<Type>,
    },
    /// Error type (e.g., error or ErrorType)
    Error,
    /// Result type with error (e.g., i32! means i32 or error)
    Result(Box<Type>),
    /// Function type (e.g., fn(i64, i64) i64)
    Function {
        /// Parameter types
        params: Vec<Type>,
        /// Return type
        return_type: Box<Type>,
    },
}

#[allow(dead_code)]
impl Type {
    #[allow(dead_code)]
    /// Get the default type for literals
    pub fn default_for_literal(literal: &str) -> Type {
        if literal.parse::<i64>().is_ok() {
            Type::I64
        } else if literal.parse::<u64>().is_ok() {
            Type::U64
        } else {
            Type::F64 // Fallback
        }
    }

    /// Check if this type is an integer type
    pub fn is_integer(&self) -> bool {
        matches!(
            self,
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

    /// Check if this type is a signed integer type
    pub fn is_signed_integer(&self) -> bool {
        matches!(self, Type::I8 | Type::I16 | Type::I32 | Type::I64)
    }

    /// Check if this type is generic or contains generic parameters
    pub fn is_generic(&self) -> bool {
        match self {
            Type::GenericParam(_) => true,
            Type::Pointer(inner) | Type::Option(inner) | Type::Result(inner) => inner.is_generic(),
            Type::Array { element_type, .. } => element_type.is_generic(),
            Type::Tuple(types) => types.iter().any(|t| t.is_generic()),
            Type::Custom { generic_args, .. } => generic_args.iter().any(|t| t.is_generic()),
            Type::Function {
                params,
                return_type,
            } => params.iter().any(|t| t.is_generic()) || return_type.is_generic(),
            _ => false,
        }
    }

    /// Check if this type is a float type
    pub fn is_float(&self) -> bool {
        matches!(self, Type::F32 | Type::F64)
    }

    /// Check whether an integer literal can be represented by this type
    pub fn can_represent_int_literal(&self, value: i64) -> bool {
        match self {
            Type::I8 => i8::try_from(value).is_ok(),
            Type::I16 => i16::try_from(value).is_ok(),
            Type::I32 => i32::try_from(value).is_ok(),
            Type::I64 => true,
            Type::U8 => u8::try_from(value).is_ok(),
            Type::U16 => u16::try_from(value).is_ok(),
            Type::U32 => u32::try_from(value).is_ok(),
            Type::U64 => value >= 0,
            _ => false,
        }
    }

    /// Recursively replace `SelfType` and `Custom("Self")` with the given struct name and optional generic arguments
    pub fn replace_self(&mut self, struct_name: &str) {
        self.replace_self_with_args(struct_name, &[]);
    }

    pub fn replace_self_with_args(&mut self, struct_name: &str, generic_args: &[Type]) {
        match self {
            Type::RawPtr => {}
            Type::SelfType => {
                *self = Type::Custom {
                    name: struct_name.to_string(),
                    generic_args: generic_args.to_vec(),
                    is_exported: false,
                };
            }
            Type::Custom {
                name,
                generic_args: args,
                ..
            } if name == "Self" => {
                eprintln!(
                    "DEBUG replace_self_with_args Custom(Self): struct_name={}, generic_args={:?}",
                    struct_name, generic_args
                );
                *name = struct_name.to_string();
                *args = generic_args.to_vec();
            }
            Type::Pointer(inner) | Type::Option(inner) | Type::Result(inner) => {
                inner.replace_self_with_args(struct_name, generic_args);
            }
            Type::Tuple(types) => {
                for t in types {
                    t.replace_self_with_args(struct_name, generic_args);
                }
            }
            Type::Array { element_type, .. } => {
                element_type.replace_self_with_args(struct_name, generic_args);
            }
            Type::Function {
                params,
                return_type,
            } => {
                for param in params {
                    param.replace_self_with_args(struct_name, generic_args);
                }
                return_type.replace_self_with_args(struct_name, generic_args);
            }
            Type::Custom {
                generic_args: args, ..
            } => {
                for arg in args {
                    arg.replace_self_with_args(struct_name, generic_args);
                }
            }
            _ => {}
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

    /// Check if this is a Result type (can return error)
    pub fn is_result(&self) -> bool {
        matches!(self, Type::Result(_))
    }

    /// Get the inner type of a Result type
    pub fn result_inner(&self) -> Option<&Type> {
        match self {
            Type::Result(inner) => Some(inner),
            _ => None,
        }
    }

    /// Check if this type is void! (Result where inner is Void)
    pub fn is_void_result(&self) -> bool {
        match self {
            Type::Result(inner) => inner.as_ref() == &Type::Void,
            _ => false,
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
            Type::F32 => write!(f, "f32"),
            Type::F64 => write!(f, "f64"),
            Type::ImmInt => write!(f, "imm_int"),
            Type::ImmFloat => write!(f, "imm_float"),
            Type::Bool => write!(f, "bool"),
            Type::Void => write!(f, "void"),
            Type::RawPtr => write!(f, "rawptr"),
            Type::SelfType => write!(f, "Self"),
            Type::Pointer(inner) => write!(f, "*{}", inner),
            Type::Option(inner) => write!(f, "?{}", inner),
            Type::Tuple(types) => {
                let type_strs: Vec<String> = types.iter().map(|t| t.to_string()).collect();
                write!(f, "({})", &type_strs.join(", "))
            }
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
            Type::Array { size, element_type } => match size {
                Some(s) => write!(f, "[{}]{}", s, element_type),
                None => write!(f, "[]{}", element_type),
            },
            Type::Error => write!(f, "error"),
            Type::Result(inner) => write!(f, "{}!", inner),
            Type::Function {
                params,
                return_type,
            } => {
                let params_str: Vec<String> = params.iter().map(|t| t.to_string()).collect();
                write!(f, "fn({}) {}", params_str.join(", "), return_type)
            }
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
    And, // Logical And (&&)
    Or,  // Logical Or (||)
    /// Range operator (..)
    Range,
    /// Bitwise operators
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
}

impl BinaryOp {
    #[allow(dead_code)]
    /// Get the precedence of the binary operator (higher = binds tighter)
    pub fn precedence(self) -> u8 {
        match self {
            BinaryOp::Or => 1,
            BinaryOp::And => 2,
            BinaryOp::BitOr => 3,
            BinaryOp::BitXor => 4,
            BinaryOp::BitAnd => 5,
            BinaryOp::Eq | BinaryOp::Ne => 6,
            BinaryOp::Lt | BinaryOp::Gt | BinaryOp::Le | BinaryOp::Ge => 7,
            BinaryOp::Shl | BinaryOp::Shr => 8,
            BinaryOp::Add | BinaryOp::Sub => 9,
            BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => 10,
            BinaryOp::Range => 11,
        }
    }
}

impl std::fmt::Display for BinaryOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BinaryOp::Add => write!(f, "+"),
            BinaryOp::Sub => write!(f, "-"),
            BinaryOp::Mul => write!(f, "*"),
            BinaryOp::Div => write!(f, "/"),
            BinaryOp::Mod => write!(f, "%"),
            BinaryOp::Eq => write!(f, "=="),
            BinaryOp::Ne => write!(f, "!="),
            BinaryOp::Lt => write!(f, "<"),
            BinaryOp::Gt => write!(f, ">"),
            BinaryOp::Le => write!(f, "<="),
            BinaryOp::Ge => write!(f, ">="),
            BinaryOp::And => write!(f, "&&"),
            BinaryOp::Or => write!(f, "||"),
            BinaryOp::Range => write!(f, ".."),
            BinaryOp::BitAnd => write!(f, "&"),
            BinaryOp::BitOr => write!(f, "|"),
            BinaryOp::BitXor => write!(f, "^"),
            BinaryOp::Shl => write!(f, "<<"),
            BinaryOp::Shr => write!(f, ">>"),
        }
    }
}

/// Unary operators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum UnaryOp {
    Neg, // - (negation)
    Pos, // + (positive)
    Not, // ! (logical not)
    Ref, // & (reference)
}

impl std::fmt::Display for UnaryOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UnaryOp::Neg => write!(f, "-"),
            UnaryOp::Pos => write!(f, "+"),
            UnaryOp::Not => write!(f, "!"),
            UnaryOp::Ref => write!(f, "&"),
        }
    }
}

/// Assignment operators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    Assign,
    AddAssign,
    SubAssign,
    MulAssign,
    DivAssign,
    ModAssign,
    AndAssign,
    OrAssign,
    XorAssign,
    ShlAssign,
    ShrAssign,
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

/// Kind of member access
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberAccessKind {
    Unknown,
    Package,      // io.println, std.SomeType
    StructField,  // point.x
    StructMethod, // point.move()
    EnumMember,   // Color.Red
    ErrorMember,  // Fail.NotFound
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
    pub interface_impls: Vec<InterfaceImpl>,
    pub visibility: Visibility,
    /// Generic type parameters (e.g., T, U)
    pub generic_params: Vec<String>,
    pub span: Span,
}

/// Interface definition
#[derive(Debug, Clone)]
pub struct InterfaceDef {
    pub name: String,
    pub methods: Vec<FnDef>,
    /// Composed interfaces included by name.
    pub composed_interfaces: Vec<String>,
    pub visibility: Visibility,
    pub span: Span,
}

/// Interface implementation block inside a struct
#[derive(Debug, Clone)]
pub struct InterfaceImpl {
    pub interface_name: String,
    pub methods: Vec<FnDef>,
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

/// Error variant
#[derive(Debug, Clone)]
pub struct ErrorVariant {
    pub name: String,
    pub associated_types: Vec<Type>,
    pub visibility: Visibility,
}

/// Error definition
#[derive(Debug, Clone)]
pub struct ErrorDef {
    pub name: String,
    pub variants: Vec<ErrorVariant>,
    pub visibility: Visibility,
    pub span: Span,
}

/// Switch case definition
#[derive(Debug, Clone)]
pub struct SwitchCase {
    pub patterns: Vec<Expr>,
    /// Optional capture variable (e.g., case Enum.Variant => |payload| { ... })
    pub capture: Option<String>,
    pub body: Stmt,
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
    /// Float literal (f64)
    Float(f64, Span),
    /// Boolean literal
    Bool(bool, Span),
    /// String literal
    String(String, Span),
    /// Character literal
    Char(char, Span),
    /// Null literal
    Null(Span),
    /// Tuple literal (e.g., (1, 2, 3))
    Tuple(Vec<Expr>, Span),
    /// Tuple index access (e.g., variable.0, variable.1)
    TupleIndex {
        /// The tuple expression
        tuple: Box<Expr>,
        /// The index to access (0, 1, 2, ...)
        index: usize,
        span: Span,
    },
    /// Array/Slice indexing or range access (e.g., a[i], a[i..j])
    Index {
        /// The array/slice expression
        object: Box<Expr>,
        /// The index expression (can be a range expression for slicing)
        index: Box<Expr>,
        span: Span,
    },
    /// Identifiers (e.g., variable names)
    Ident(String, Span),
    /// Array literal (e.g., [1, 2, 3]) or typed array (e.g., [3]u8{1, 2, 3})
    /// For typed arrays, `ty` contains the element type (e.g., u8 for [3]u8{...})
    Array(Vec<Expr>, Option<Type>, Span),
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
        /// Generic type arguments (e.g., T in add<T>(v))
        generic_args: Vec<Type>,
        span: Span,
    },
    /// If expression
    If {
        condition: Box<Expr>,
        /// Optional capture variable (e.g., if (opt) |data| { ... })
        capture: Option<String>,
        then_branch: Box<Expr>,
        else_branch: Box<Expr>,
        span: Span,
    },
    /// Block expression (a sequence of statements that evaluates to a value)
    Block { stmts: Vec<Stmt>, span: Span },
    /// Member access (e.g., Status.Todo or object.field)
    MemberAccess {
        object: Box<Expr>,
        member: String,
        kind: MemberAccessKind,
        span: Span,
    },
    /// Struct literal (e.g., Base{ name, age, married })
    Struct {
        name: String,
        fields: Vec<(String, Expr)>,
        /// Generic type arguments (e.g., T in Compose<T>{...})
        generic_args: Vec<Type>,
        span: Span,
    },
    /// Try expression (e.g., try some_function())
    Try { expr: Box<Expr>, span: Span },
    /// Catch expression (e.g., expr catch |e| { ... })
    Catch {
        expr: Box<Expr>,
        /// Optional capture variable for the error
        error_var: Option<String>,
        body: Box<Expr>,
        span: Span,
    },
    /// Type cast expression (e.g., i32(expr), f64(value))
    /// This is used for explicit type conversions between primitive types
    Cast {
        /// The target type to cast to
        target_type: Type,
        /// The expression to cast
        expr: Box<Expr>,
        span: Span,
    },
    /// Pointer dereference (e.g., ptr.*)
    Dereference {
        /// The pointer expression to dereference
        expr: Box<Expr>,
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
        /// For tuple destructuring: names[i] = Some(name) to bind, None to ignore
        /// e.g., const (a, _, c) = tuple; => names = [Some("a"), None, Some("c")]
        names: Option<Vec<Option<String>>>,
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
        /// Optional capture variable (e.g., if (opt) |data| { ... })
        capture: Option<String>,
        then_branch: Box<Stmt>,
        else_branch: Option<Box<Stmt>>,
        span: Span,
    },
    /// For loop
    For {
        /// Optional label (e.g., outer: for ...)
        label: Option<String>,
        /// Optional index or element variable (e.g., for i in range)
        var_name: Option<String>,
        iterable: Expr,
        /// Optional capture variable for iterators
        capture: Option<String>,
        /// Optional index variable for array iteration (e.g., |k, v|)
        index_var: Option<String>,
        body: Box<Stmt>,
        span: Span,
    },
    /// Switch statement
    Switch {
        condition: Expr,
        cases: Vec<SwitchCase>,
        span: Span,
    },
    /// Defer statement (executes on scope exit)
    Defer { stmt: Box<Stmt>, span: Span },
    /// Defer! statement (executes only on error in try statement)
    DeferBang { stmt: Box<Stmt>, span: Span },
    /// Break statement (exits a loop)
    Break { label: Option<String>, span: Span },
    /// Continue statement (skips to next iteration)
    Continue { label: Option<String>, span: Span },
}

/// Function definition AST node
#[derive(Debug, Clone)]
pub struct FnDef {
    pub name: String,
    pub visibility: Visibility,
    pub params: Vec<FnParam>,
    pub return_ty: Type,
    pub body: Vec<Stmt>,
    /// Generic type parameters (e.g., T, U)
    pub generic_params: Vec<String>,
    /// Interface constraints for generic params (e.g., T: Service)
    pub generic_constraints: Vec<(String, String)>,
    pub span: Span,
}

/// Function parameter
#[derive(Debug, Clone)]
pub struct FnParam {
    pub name: String,
    pub ty: Type,
}

/// External C function declaration (FFI)
#[derive(Debug, Clone)]
pub struct ExternalFnDef {
    pub name: String,
    pub visibility: Visibility,
    pub params: Vec<FnParam>,
    pub return_ty: Type,
    pub span: Span,
}

/// Program AST node (root of the tree)
#[derive(Debug, Clone)]
pub struct Program {
    pub functions: Vec<FnDef>,
    pub external_functions: Vec<ExternalFnDef>,
    pub structs: Vec<StructDef>,
    pub interfaces: Vec<InterfaceDef>,
    pub enums: Vec<EnumDef>,
    pub errors: Vec<ErrorDef>,
    pub imports: Vec<(Option<String>, String)>, // (alias, package_name)
}

/// Visitor trait for AST traversal
#[allow(dead_code)]
pub trait ASTVisitor<T> {
    fn visit_expr(&mut self, expr: &Expr) -> T;
    fn visit_stmt(&mut self, stmt: &Stmt) -> T;
    fn visit_program(&mut self, program: &Program) -> T;
}

/// Helper to create spans (placeholder implementation)
#[allow(dead_code)]
pub fn span(start: usize, end: usize) -> Span {
    Span { start, end }
}

/// Trait for pretty-printing the AST in a tree structure without span information
pub trait AstDump {
    fn dump(&self, indent: usize);
}

pub fn print_indent(indent: usize) {
    for _ in 0..indent {
        print!("  ");
    }
}

impl AstDump for Program {
    fn dump(&self, indent: usize) {
        print_indent(indent);
        println!("Program");

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
        for i in &self.interfaces {
            i.dump(indent + 1);
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

impl AstDump for FnDef {
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
        let constraints = if self.generic_constraints.is_empty() {
            String::new()
        } else {
            format!(
                " where {}",
                self.generic_constraints
                    .iter()
                    .map(|(param, interface)| format!("{}: {}", param, interface))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        println!(
            "FnDef: {}{}{}{} -> {}",
            vis, self.name, generics, constraints, self.return_ty
        );

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

impl AstDump for ExternalFnDef {
    fn dump(&self, indent: usize) {
        print_indent(indent);
        let vis = if self.visibility.is_public() {
            "pub "
        } else {
            ""
        };
        println!("ExternalFnDef: {}{} -> {}", vis, self.name, self.return_ty);

        if !self.params.is_empty() {
            print_indent(indent + 1);
            println!("Params:");
            for p in &self.params {
                print_indent(indent + 2);
                println!("{}: {}", p.name, p.ty);
            }
        }
    }
}

impl AstDump for StructDef {
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
        for interface_impl in &self.interface_impls {
            print_indent(indent + 1);
            println!("Impl {}", interface_impl.interface_name);
            for method in &interface_impl.methods {
                method.dump(indent + 2);
            }
        }
    }
}

impl AstDump for InterfaceDef {
    fn dump(&self, indent: usize) {
        print_indent(indent);
        let vis = if self.visibility.is_public() {
            "pub "
        } else {
            ""
        };
        println!("InterfaceDef: {}{}", vis, self.name);
        for composed in &self.composed_interfaces {
            print_indent(indent + 1);
            println!("Compose: {}", composed);
        }
        for method in &self.methods {
            method.dump(indent + 1);
        }
    }
}

impl AstDump for EnumDef {
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

impl AstDump for ErrorDef {
    fn dump(&self, indent: usize) {
        print_indent(indent);
        let vis = if self.visibility.is_public() {
            "pub "
        } else {
            ""
        };
        println!("ErrorDef: {}{}", vis, self.name);

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

impl AstDump for Stmt {
    fn dump(&self, indent: usize) {
        print_indent(indent);
        match self {
            Stmt::Expr { expr, .. } => {
                println!("Stmt::Expr");
                expr.dump(indent + 1);
            }
            Stmt::Import { packages, .. } => {
                println!("Stmt::Import: {:?}", packages);
            }
            Stmt::Let {
                mutability,
                name,
                names,
                ty,
                value,
                visibility,
                ..
            } => {
                let mut_str = match mutability {
                    Mutability::Var => "var",
                    Mutability::Const => "const",
                };
                let vis = if visibility.is_public() { "pub " } else { "" };
                let name_str = if let Some(ns) = names {
                    format!("{:?}", ns)
                } else {
                    name.clone()
                };
                let ty_str = if let Some(t) = ty {
                    format!(": {}", t)
                } else {
                    "".to_string()
                };
                println!("Stmt::Let: {}{} {}{}", vis, mut_str, name_str, ty_str);
                if let Some(v) = value {
                    print_indent(indent + 1);
                    println!("Value:");
                    v.dump(indent + 2);
                }
            }
            Stmt::Assign {
                target, op, value, ..
            } => {
                println!("Stmt::Assign");
                print_indent(indent + 1);
                println!("Target: {}", target);
                print_indent(indent + 1);
                println!("Op: {:?}", op);
                print_indent(indent + 1);
                println!("Value:");
                value.dump(indent + 2);
            }
            Stmt::Return { value, .. } => {
                println!("Stmt::Return");
                if let Some(v) = value {
                    print_indent(indent + 1);
                    println!("Value:");
                    v.dump(indent + 2);
                }
            }
            Stmt::Block { stmts, .. } => {
                println!("Stmt::Block");
                for s in stmts {
                    s.dump(indent + 1);
                }
            }
            Stmt::If {
                condition,
                capture,
                then_branch,
                else_branch,
                ..
            } => {
                println!("Stmt::If");
                if let Some(c) = capture {
                    print_indent(indent + 1);
                    println!("Capture: {}", c);
                }
                print_indent(indent + 1);
                println!("Condition:");
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
            Stmt::For {
                label,
                var_name,
                iterable,
                capture,
                index_var,
                body,
                ..
            } => {
                println!("Stmt::For");
                if let Some(l) = label {
                    print_indent(indent + 1);
                    println!("Label: {}", l);
                }
                if let Some(v) = var_name {
                    print_indent(indent + 1);
                    println!("Var: {}", v);
                }
                if let Some(i) = index_var {
                    print_indent(indent + 1);
                    println!("Index: {}", i);
                }
                if let Some(c) = capture {
                    print_indent(indent + 1);
                    println!("Capture: {}", c);
                }
                print_indent(indent + 1);
                println!("Iterable:");
                iterable.dump(indent + 2);
                print_indent(indent + 1);
                println!("Body:");
                body.dump(indent + 2);
            }
            Stmt::Switch {
                condition, cases, ..
            } => {
                println!("Stmt::Switch");
                print_indent(indent + 1);
                println!("Condition:");
                condition.dump(indent + 2);
                for (i, case) in cases.iter().enumerate() {
                    print_indent(indent + 1);
                    println!("Case {}:", i);
                    if let Some(c) = &case.capture {
                        print_indent(indent + 2);
                        println!("Capture: {}", c);
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
            Stmt::Defer { stmt, .. } => {
                println!("Stmt::Defer");
                stmt.dump(indent + 1);
            }
            Stmt::DeferBang { stmt, .. } => {
                println!("Stmt::Defer!");
                stmt.dump(indent + 1);
            }
            Stmt::Break { label, .. } => {
                let lbl = if let Some(l) = label {
                    format!(" {}", l)
                } else {
                    "".to_string()
                };
                println!("Stmt::Break{}", lbl);
            }
            Stmt::Continue { label, .. } => {
                let lbl = if let Some(l) = label {
                    format!(" {}", l)
                } else {
                    "".to_string()
                };
                println!("Stmt::Continue{}", lbl);
            }
        }
    }
}

impl AstDump for Expr {
    fn dump(&self, indent: usize) {
        print_indent(indent);
        match self {
            Expr::Int(val, _) => println!("Expr::{}({})", "Int", val),
            Expr::Float(val, _) => println!("Expr::{}({})", "Float", val),
            Expr::Bool(val, _) => println!("Expr::{}({})", "Bool", val),
            Expr::String(val, _) => println!("Expr::String(\"{}\")", val),
            Expr::Char(val, _) => println!("Expr::Char('{}')", val),
            Expr::Null(_) => println!("Expr::Null"),
            Expr::Tuple(exprs, _) => {
                println!("Expr::Tuple");
                for e in exprs {
                    e.dump(indent + 1);
                }
            }
            Expr::TupleIndex { tuple, index, .. } => {
                println!("Expr::TupleIndex: .{}", index);
                tuple.dump(indent + 1);
            }
            Expr::Index { object, index, .. } => {
                println!("Expr::Index: []");
                object.dump(indent + 1);
                index.dump(indent + 1);
            }
            Expr::Ident(name, _) => println!("Expr::Ident({})", name),
            Expr::Array(exprs, _, _) => {
                println!("Expr::Array");
                for e in exprs {
                    e.dump(indent + 1);
                }
            }
            Expr::Binary {
                op, left, right, ..
            } => {
                println!("Expr::Binary: {:?}", op);
                left.dump(indent + 1);
                right.dump(indent + 1);
            }
            Expr::Unary { op, expr, .. } => {
                println!("Expr::Unary: {:?}", op);
                expr.dump(indent + 1);
            }
            Expr::Call {
                name,
                namespace,
                args,
                generic_args,
                ..
            } => {
                let ns = if let Some(n) = namespace {
                    format!("{}::", n)
                } else {
                    "".to_string()
                };
                let generics = if generic_args.is_empty() {
                    "".to_string()
                } else {
                    let args: Vec<String> = generic_args.iter().map(|a| a.to_string()).collect();
                    format!("<{}>", args.join(", "))
                };
                println!("Expr::Call: {}{}{}", ns, name, generics);
                for a in args {
                    a.dump(indent + 1);
                }
            }
            Expr::If {
                condition,
                capture,
                then_branch,
                else_branch,
                ..
            } => {
                println!("Expr::If");
                if let Some(c) = capture {
                    print_indent(indent + 1);
                    println!("Capture: {}", c);
                }
                print_indent(indent + 1);
                println!("Condition:");
                condition.dump(indent + 2);
                print_indent(indent + 1);
                println!("Then:");
                then_branch.dump(indent + 2);
                print_indent(indent + 1);
                println!("Else:");
                else_branch.dump(indent + 2);
            }
            Expr::Block { stmts, .. } => {
                println!("Expr::Block");
                for s in stmts {
                    s.dump(indent + 1);
                }
            }
            Expr::MemberAccess {
                object,
                member,
                kind,
                ..
            } => {
                let kind_str = match kind {
                    MemberAccessKind::Unknown => "",
                    MemberAccessKind::Package => " (package)",
                    MemberAccessKind::StructField => " (field)",
                    MemberAccessKind::StructMethod => " (method)",
                    MemberAccessKind::EnumMember => " (enum)",
                    MemberAccessKind::ErrorMember => " (error)",
                };
                println!("Expr::MemberAccess: .{}{}", member, kind_str);
                object.dump(indent + 1);
            }
            Expr::Struct {
                name,
                fields,
                generic_args,
                ..
            } => {
                let generics = if generic_args.is_empty() {
                    "".to_string()
                } else {
                    let args: Vec<String> = generic_args.iter().map(|a| a.to_string()).collect();
                    format!("<{}>", args.join(", "))
                };
                println!("Expr::Struct: {}{}", name, generics);
                for (fname, fval) in fields {
                    print_indent(indent + 1);
                    println!("Field: {}:", fname);
                    fval.dump(indent + 2);
                }
            }
            Expr::Try { expr, .. } => {
                println!("Expr::Try");
                expr.dump(indent + 1);
            }
            Expr::Catch {
                expr,
                error_var,
                body,
                ..
            } => {
                println!("Expr::Catch");
                if let Some(v) = error_var {
                    print_indent(indent + 1);
                    println!("Capture: {}", v);
                }
                print_indent(indent + 1);
                println!("Value:");
                expr.dump(indent + 2);
                print_indent(indent + 1);
                println!("Body:");
                body.dump(indent + 2);
            }
            Expr::Cast {
                target_type, expr, ..
            } => {
                println!("Expr::Cast to {}", target_type);
                expr.dump(indent + 1);
            }
            Expr::Dereference { expr, .. } => {
                println!("Expr::Dereference");
                expr.dump(indent + 1);
            }
        }
    }
}
