//! # Type Inference Engine
//!
//! This module provides type inference for the AST, producing a type-annotated AST
//! where every expression has its inferred type explicitly stored.

use crate::ast::Visibility;
use crate::ast::{AssignOp, BinaryOp, Expr, FnDef, FnParam, Program, Span, Stmt, Type, UnaryOp};
use crate::sema::error::{AnalysisError, AnalysisResult};
use crate::sema::symbol::{ConstantValue, Symbol, SymbolTable};
use std::collections::{HashMap, HashSet};

// ============================================================================
// Type-Annotated AST Nodes
// ============================================================================

/// Type-annotated expression with its inferred type
#[derive(Debug, Clone)]
#[allow(unused)]
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
    /// Array/Slice indexing or range access
    Index {
        object: Box<TypedExpr>,
        index: Box<TypedExpr>,
    },
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
        /// Target type for method calls - used to resolve method name to monomorphized version
        target_ty: Option<Type>,
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
    /// Type cast expression
    Cast {
        target_type: Type,
        expr: Box<TypedExpr>,
    },
    /// Pointer dereference
    Dereference { expr: Box<TypedExpr> },
    /// Intrinsic function call
    Intrinsic { name: String, args: Vec<TypedExpr> },
    /// Type literal used as an argument to intrinsic functions
    TypeLiteral(Type),
}

#[allow(unused)]
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
#[allow(unused)]
pub struct TypedStmt {
    pub stmt: TypedStmtKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
#[allow(unused)]
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
    Assign {
        target: String,
        op: AssignOp,
        value: TypedExpr,
    },
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
    /// Continue statement
    Continue { label: Option<String> },
}

#[derive(Debug, Clone)]
pub struct TypedSwitchCase {
    pub patterns: Vec<TypedExpr>,
    pub capture: Option<String>,
    pub body: TypedStmt,
}

/// Type-annotated function definition
#[derive(Debug, Clone)]
#[allow(unused)]
pub struct TypedFnDef {
    pub name: String,
    pub original_name: Option<String>,
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

fn destructured_binding_type(aggregate_ty: &Type, index: usize) -> Type {
    match aggregate_ty {
        Type::Tuple(types) => types.get(index).cloned().unwrap_or(Type::I64),
        _ => aggregate_ty.clone(),
    }
}

fn destructured_binding_type_checked(
    aggregate_ty: &Type,
    index: usize,
    span: &Span,
) -> AnalysisResult<Type> {
    match aggregate_ty {
        Type::Tuple(types) => types.get(index).cloned().ok_or_else(|| {
            AnalysisError::new_with_span(
                &format!(
                    "Tuple destructuring expected at least {} elements, but found {}",
                    index + 1,
                    types.len()
                ),
                span,
            )
            .with_module("infer")
        }),
        _ => Err(AnalysisError::new_with_span(
            &format!("Cannot destructure non-tuple type {}", aggregate_ty),
            span,
        )
        .with_module("infer")),
    }
}

fn for_binding_type(iterable: &Expr, iterable_ty: &Type) -> AnalysisResult<Option<Type>> {
    if matches!(iterable, Expr::Null(_)) {
        return Ok(None);
    }

    match iterable_ty {
        Type::Array { element_type, .. } => Ok(Some(element_type.as_ref().clone())),
        Type::Option(inner_ty) => Ok(Some(inner_ty.as_ref().clone())),
        Type::Tuple(types) if types.len() == 2 => Ok(types.first().cloned()),
        Type::Bool => Ok(None),
        _ => Err(AnalysisError::new(&format!(
            "For loop iterable of type {} is not supported",
            iterable_ty
        ))
        .with_module("infer")),
    }
}

fn for_index_type(iterable: &Expr, iterable_ty: &Type) -> AnalysisResult<Option<Type>> {
    if matches!(iterable, Expr::Null(_)) {
        return Ok(None);
    }

    match iterable_ty {
        Type::Array { .. } => Ok(Some(Type::I64)),
        Type::Option(_) | Type::Tuple(_) | Type::Bool => Ok(None),
        _ => Err(AnalysisError::new(&format!(
            "For loop iterable of type {} is not supported",
            iterable_ty
        ))
        .with_module("infer")),
    }
}

fn custom_type_name(ty: &Type) -> Option<&str> {
    match ty {
        Type::Custom { name, .. } => Some(name.as_str()),
        Type::Pointer(inner) => custom_type_name(inner),
        _ => None,
    }
}

fn align_to_u64(value: u64, align: u64) -> AnalysisResult<u64> {
    if align == 0 {
        return Err(AnalysisError::new("Alignment must be greater than zero").with_module("infer"));
    }
    let remainder = value % align;
    if remainder == 0 {
        return Ok(value);
    }
    value.checked_add(align - remainder).ok_or_else(|| {
        AnalysisError::new("Type metadata overflow while aligning").with_module("infer")
    })
}

fn format_typed_binding_names(names: &[Option<String>], ty: &Type) -> String {
    let bindings: Vec<String> = names
        .iter()
        .enumerate()
        .map(|(index, name_opt)| match name_opt {
            Some(name) => format!("{}: {}", name, destructured_binding_type(ty, index)),
            None => "_".to_string(),
        })
        .collect();

    format!("[{}]", bindings.join(", "))
}

/// Type-annotated struct definition
#[derive(Debug, Clone)]
#[allow(unused)]
pub struct TypedStructDef {
    pub name: String,
    pub fields: Vec<crate::ast::StructField>,
    pub methods: Vec<TypedFnDef>,
    pub visibility: Visibility,
    pub generic_params: Vec<String>,
    pub span: Span,
}

/// Type-annotated interface definition
#[derive(Debug, Clone)]
#[allow(unused)]
pub struct TypedInterfaceDef {
    pub name: String,
    pub methods: Vec<TypedFnDef>,
    /// Composed interfaces included by name.
    pub composed_interfaces: Vec<String>,
    pub visibility: Visibility,
    pub span: Span,
}

/// Type-annotated enum definition
#[derive(Debug, Clone)]
#[allow(unused)]
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
#[allow(unused)]
pub struct TypedErrorDef {
    pub name: String,
    pub variants: Vec<crate::ast::ErrorVariant>,
    pub visibility: Visibility,
    pub span: Span,
}

/// Type-annotated program
#[derive(Debug, Clone)]
pub struct TypedProgram {
    pub functions: Vec<TypedFnDef>,
    pub external_functions: Vec<TypedFnDef>,
    pub structs: Vec<TypedStructDef>,
    pub interfaces: Vec<TypedInterfaceDef>,
    pub enums: Vec<TypedEnumDef>,
    pub errors: Vec<TypedErrorDef>,
    pub imports: Vec<(Option<String>, String)>,
    /// Original AST-level function definitions (including monomorphized ones) for the lowering pass
    pub ast_functions: Vec<crate::ast::FnDef>,
    /// Original AST-level struct definitions (including monomorphized ones) for the lowering pass
    pub ast_structs: Vec<crate::ast::StructDef>,
}

// ============================================================================
// Type Inference Engine
// ============================================================================

/// Type inference engine that traverses the AST and infers types
pub struct TypeInferrer {
    symbol_table: SymbolTable,
    /// Map of struct name to its definition
    structs: HashMap<String, crate::ast::StructDef>,
    /// Map of interface name to its definition
    interfaces: HashMap<String, crate::ast::InterfaceDef>,
    /// Map of enum name to its definition
    enums: HashMap<String, crate::ast::EnumDef>,
    /// Map of error name to its definition
    errors: HashMap<String, crate::ast::ErrorDef>,
    /// Map of function name to its definition
    functions: HashMap<String, crate::ast::FnDef>,
    /// Expected type for the current expression being inferred
    expected_type: Option<Type>,
    /// Expected return type for the current function being inferred
    expected_return_type: Option<Type>,
    /// Current type parameter mappings (for monomorphization)
    type_mappings: HashMap<String, Type>,
    /// Monomorphized function instantiations: (original_name, type_args) -> mangled_name
    fn_instantiations: HashMap<(String, Vec<Type>), String>,
    /// Monomorphized struct instantiations: (original_name, type_args) -> mangled_name
    struct_instantiations: HashMap<(String, Vec<Type>), String>,
    /// New functions created during monomorphization
    new_functions: Vec<TypedFnDef>,
    /// New structs created during monomorphization
    new_structs: Vec<TypedStructDef>,
    /// New AST-level functions (monomorphized) for lowering pass
    new_ast_functions: Vec<crate::ast::FnDef>,
    /// New AST-level structs (monomorphized) for lowering pass
    new_ast_structs: Vec<crate::ast::StructDef>,
    /// Track imported packages to validate package access
    imports: Vec<(Option<String>, String)>,
    /// Active interface constraints for the function currently being inferred
    current_generic_constraints: HashMap<String, Vec<String>>,
    /// Track if current function body contains a try expression
    has_try_expression: bool,
    /// Span of the first try expression in the function body
    try_expression_span: Option<Span>,
}

impl TypeInferrer {
    /// Create a new type inferrer
    pub fn new(
        symbol_table: SymbolTable,
        structs: HashMap<String, crate::ast::StructDef>,
        interfaces: HashMap<String, crate::ast::InterfaceDef>,
        enums: HashMap<String, crate::ast::EnumDef>,
        errors: HashMap<String, crate::ast::ErrorDef>,
        functions: HashMap<String, crate::ast::FnDef>,
        imports: Vec<(Option<String>, String)>,
    ) -> Self {
        TypeInferrer {
            symbol_table,
            structs,
            interfaces,
            enums,
            errors,
            functions,
            expected_type: None,
            expected_return_type: None,
            type_mappings: HashMap::new(),
            fn_instantiations: HashMap::new(),
            struct_instantiations: HashMap::new(),
            new_functions: Vec::new(),
            new_structs: Vec::new(),
            new_ast_functions: Vec::new(),
            new_ast_structs: Vec::new(),
            imports,
            current_generic_constraints: HashMap::new(),
            has_try_expression: false,
            try_expression_span: None,
        }
    }

    fn builtin_layout_of(&mut self, ty: &Type) -> AnalysisResult<(u64, u64)> {
        let resolved = self.substitute_type(ty, &HashMap::new());
        let mut visiting = HashSet::new();
        self.type_layout_of(&resolved, &mut visiting)
    }

    fn type_layout_of(
        &mut self,
        ty: &Type,
        visiting: &mut HashSet<String>,
    ) -> AnalysisResult<(u64, u64)> {
        match ty {
            Type::Void => Ok((0, 1)),
            Type::Bool | Type::I8 | Type::U8 => Ok((1, 1)),
            Type::I16 | Type::U16 => Ok((2, 2)),
            Type::I32 | Type::U32 | Type::F32 => Ok((4, 4)),
            Type::I64
            | Type::U64
            | Type::F64
            | Type::RawPtr
            | Type::Pointer(_)
            | Type::Function { .. }
            | Type::Error => Ok((8, 8)),
            Type::GenericParam(name) => {
                if let Some(mapped) = self.type_mappings.get(name).cloned() {
                    self.type_layout_of(&mapped, visiting)
                } else {
                    Err(AnalysisError::new(&format!(
                        "Cannot evaluate type metadata for unresolved generic parameter '{}'",
                        name
                    ))
                    .with_module("infer"))
                }
            }
            Type::Option(inner) | Type::Result(inner) => {
                let (payload_size, payload_align) = self.type_layout_of(inner, visiting)?;
                let tag_align = 1u64;
                let tag_offset = align_to_u64(payload_size, tag_align)?;
                let total = tag_offset.checked_add(1).ok_or_else(|| {
                    AnalysisError::new("Type metadata overflow while computing tagged layout")
                        .with_module("infer")
                })?;
                let align = payload_align.max(tag_align);
                Ok((align_to_u64(total, align)?, align))
            }
            Type::Const(inner) => self.type_layout_of(inner, visiting),
            Type::Tuple(types) => self.aggregate_layout_of(types, visiting),
            Type::Array { size, element_type } => match size {
                Some(count) => {
                    let (elem_size, elem_align) = self.type_layout_of(element_type, visiting)?;
                    let total = elem_size.checked_mul(*count as u64).ok_or_else(|| {
                        AnalysisError::new("Type metadata overflow while computing array size")
                            .with_module("infer")
                    })?;
                    Ok((total, elem_align))
                }
                None => Ok((8, 8)),
            },
            Type::Custom {
                name, generic_args, ..
            } => {
                if visiting.contains(name) {
                    return Err(AnalysisError::new(&format!(
                        "Recursive type layout is not supported for '{}'",
                        name
                    ))
                    .with_module("infer"));
                }
                let struct_def = self.structs.get(name).cloned().ok_or_else(|| {
                    AnalysisError::new(&format!("Unknown type '{}' in type metadata query", name))
                        .with_module("infer")
                })?;
                visiting.insert(name.clone());

                let mut mappings = HashMap::new();
                for (index, generic_name) in struct_def.generic_params.iter().enumerate() {
                    if let Some(arg) = generic_args.get(index) {
                        mappings.insert(
                            generic_name.clone(),
                            self.substitute_type(arg, &HashMap::new()),
                        );
                    }
                }

                let mut field_types = Vec::new();
                for field in &struct_def.fields {
                    field_types.push(self.substitute_type(&field.ty, &mappings));
                }

                let layout = self.aggregate_layout_of(&field_types, visiting);
                visiting.remove(name);
                layout
            }
            Type::SelfType | Type::ImmInt | Type::ImmFloat => Err(AnalysisError::new(&format!(
                "Type metadata is not available for '{}'",
                ty
            ))
            .with_module("infer")),
        }
    }

    fn aggregate_layout_of(
        &mut self,
        field_types: &[Type],
        visiting: &mut HashSet<String>,
    ) -> AnalysisResult<(u64, u64)> {
        let mut offset = 0u64;
        let mut max_align = 1u64;

        for field_ty in field_types {
            let (field_size, field_align) = self.type_layout_of(field_ty, visiting)?;
            offset = align_to_u64(offset, field_align)?;
            offset = offset.checked_add(field_size).ok_or_else(|| {
                AnalysisError::new("Type metadata overflow while computing aggregate layout")
                    .with_module("infer")
            })?;
            if field_align > max_align {
                max_align = field_align;
            }
        }

        Ok((align_to_u64(offset, max_align)?, max_align))
    }

    /// Substitute generic parameters in a type with concrete types
    fn substitute_type(&mut self, ty: &Type, mappings: &HashMap<String, Type>) -> Type {
        // Use self.type_mappings if mappings is empty but we have active mappings
        let effective_mappings = if mappings.is_empty() && !self.type_mappings.is_empty() {
            self.type_mappings.clone()
        } else {
            mappings.clone()
        };

        eprintln!(
            "DEBUG substitute_type: ty={:?}, mappings={:?}, effective_mappings={:?}",
            ty, mappings, effective_mappings
        );
        match ty {
            Type::GenericParam(name) => effective_mappings.get(name).cloned().unwrap_or(ty.clone()),
            Type::Pointer(inner) => {
                let substituted = self.substitute_type(inner, &effective_mappings);
                Type::Pointer(Box::new(substituted))
            }
            Type::Option(inner) => {
                let substituted = self.substitute_type(inner, &effective_mappings);
                Type::Option(Box::new(substituted))
            }
            Type::Result(inner) => {
                let substituted = self.substitute_type(inner, &effective_mappings);
                Type::Result(Box::new(substituted))
            }
            Type::Array { size, element_type } => {
                let substituted = self.substitute_type(element_type, &effective_mappings);
                Type::Array {
                    size: *size,
                    element_type: Box::new(substituted),
                }
            }
            Type::Tuple(types) => {
                let mut substituted = Vec::new();
                for t in types {
                    substituted.push(self.substitute_type(t, &effective_mappings));
                }
                Type::Tuple(substituted)
            }
            Type::Custom {
                name,
                generic_args,
                is_exported,
            } => {
                let mut substituted_args = Vec::new();
                for t in generic_args {
                    substituted_args.push(self.substitute_type(t, &effective_mappings));
                }

                eprintln!(
                    "DEBUG substitute_type Custom: name={}, generic_args={:?}, substituted_args={:?}",
                    name, generic_args, substituted_args
                );

                // Check if we should mangle the name (only if all args are concrete and it had generic params)
                let has_generic_params = self
                    .structs
                    .get(name)
                    .map(|s| !s.generic_params.is_empty())
                    .unwrap_or(false)
                    || self
                        .enums
                        .get(name)
                        .map(|e| !e.generic_params.is_empty())
                        .unwrap_or(false);

                eprintln!(
                    "DEBUG Custom type: name={}, has_generic_params={}, substituted_args={:?}",
                    name, has_generic_params, substituted_args
                );

                if has_generic_params
                    && !substituted_args.is_empty()
                    && substituted_args.iter().all(|t| !t.is_generic())
                {
                    let mangled = self.get_mangled_name(name, &substituted_args);

                    // Ensure the monomorphized struct is instantiated - but only if all args are concrete
                    let key = (name.clone(), substituted_args.clone());
                    eprintln!(
                        "DEBUG substitute_type Custom - inserting into struct_instantiations: key={:?}, mangled={}",
                        key, mangled
                    );
                    let all_concrete = !substituted_args.iter().any(|t| t.is_generic());
                    if all_concrete
                        && !self.struct_instantiations.contains_key(&key)
                        && self.structs.contains_key(name)
                    {
                        self.struct_instantiations.insert(key, mangled.clone());
                        // We need to be careful here: instantiate_struct calls substitute_type
                        // but only on fields, and it uses a different mappings.
                        // It should be fine as long as we don't have infinite recursion.
                        let _ = self.instantiate_struct(name, &substituted_args, &mangled);
                    }

                    Type::Custom {
                        name: mangled,
                        generic_args: Vec::new(),
                        is_exported: *is_exported,
                    }
                } else {
                    eprintln!(
                        "DEBUG NOT mangling: name={}, has_generic_params={}",
                        name, has_generic_params
                    );
                    Type::Custom {
                        name: name.clone(),
                        generic_args: substituted_args,
                        is_exported: *is_exported,
                    }
                }
            }
            Type::Function {
                params,
                return_type,
            } => {
                let mut substituted_params = Vec::new();
                for t in params {
                    substituted_params.push(self.substitute_type(t, &effective_mappings));
                }
                let substituted_return = self.substitute_type(return_type, &effective_mappings);
                Type::Function {
                    params: substituted_params,
                    return_type: Box::new(substituted_return),
                }
            }
            _ => ty.clone(),
        }
    }

    /// Set the expected type for the current expression being inferred
    fn set_expected_type(&mut self, ty: Option<Type>) {
        self.expected_type = ty;
    }

    /// Get the expected type for the current expression being inferred
    fn get_expected_type(&self) -> Option<&Type> {
        self.expected_type.as_ref()
    }

    /// Get the effective expected type including return type context
    fn get_effective_expected_type(&self) -> Option<Type> {
        // Prefer explicitly set expected type, fall back to return type
        if let Some(ref ty) = self.expected_type {
            // Don't use void or error types for type inference
            if !matches!(ty, Type::Void | Type::Error) {
                return Some(ty.clone());
            }
        }
        if let Some(ref ret_ty) = self.expected_return_type {
            // Don't use void or error types for type inference
            if !matches!(ret_ty, Type::Void | Type::Error) {
                return Some(ret_ty.clone());
            }
        }
        None
    }

    /// Validate io.println format string arguments
    fn validate_io_println_format(
        &self,
        format_str: &str,
        args: &[TypedExpr],
        span: &Span,
    ) -> AnalysisResult<()> {
        // Parse format string to extract placeholders
        let placeholders = self.parse_format_placeholders(format_str);

        // Check number of arguments matches placeholders
        if placeholders.len() != args.len() {
            return Err(AnalysisError::new_with_span(
                &format!(
                    "io.println format string has {} placeholders but got {} arguments",
                    placeholders.len(),
                    args.len()
                ),
                span,
            )
            .with_module("infer"));
        }

        // Check each argument type matches the placeholder
        for (idx, (placeholder, arg)) in placeholders.iter().zip(args.iter()).enumerate() {
            match placeholder.as_str() {
                "s" => {
                    // {s} requires u8 array or slice
                    if !self.is_u8_array_or_slice(&arg.ty) {
                        return Err(AnalysisError::new_with_span(
                            &format!(
                                "io.println placeholder #{} {{}} requires u8 array/slice, got {:?}",
                                idx + 1,
                                arg.ty
                            ),
                            &arg.span,
                        )
                        .with_module("infer"));
                    }
                }
                "d" => {
                    // {d} requires integer type
                    if !self.is_integer_type(&arg.ty) {
                        return Err(AnalysisError::new_with_span(
                            &format!(
                                "io.println placeholder #{} {{d}} requires integer type (i8/u8, i16/u16, i32/u32, i64/u64), got {:?}",
                                idx + 1,
                                arg.ty
                            ),
                            &arg.span,
                        ).with_module("infer"));
                    }
                }
                "f" => {
                    // {f} requires float type
                    if !self.is_float_type(&arg.ty) {
                        return Err(AnalysisError::new_with_span(
                            &format!(
                                "io.println placeholder #{} {{f}} requires float type (f32/f64), got {:?}",
                                idx + 1,
                                arg.ty
                            ),
                            &arg.span,
                        ).with_module("infer"));
                    }
                }
                "x" => {
                    // {x} requires integer type
                    if !self.is_integer_type(&arg.ty) {
                        return Err(AnalysisError::new_with_span(
                            &format!(
                                "io.println placeholder #{} {{x}} requires integer type (i8/u8, i16/u16, i32/u32, i64/u64), got {:?}",
                                idx + 1,
                                arg.ty
                            ),
                            &arg.span,
                        ).with_module("infer"));
                    }
                }
                "X" => {
                    // {X} requires integer type (uppercase hex)
                    if !self.is_integer_type(&arg.ty) {
                        return Err(AnalysisError::new_with_span(
                            &format!(
                                "io.println placeholder #{} {{X}} requires integer type (i8/u8, i16/u16, i32/u32, i64/u64), got {:?}",
                                idx + 1,
                                arg.ty
                            ),
                            &arg.span,
                        ).with_module("infer"));
                    }
                }
                "b" => {
                    // {b} requires boolean type
                    if !self.is_bool_type(&arg.ty) {
                        return Err(AnalysisError::new_with_span(
                            &format!(
                                "io.println placeholder #{} {{b}} requires boolean type, got {:?}",
                                idx + 1,
                                arg.ty
                            ),
                            &arg.span,
                        )
                        .with_module("infer"));
                    }
                }
                _ => {
                    // Unknown placeholder - ignore
                }
            }
        }

        Ok(())
    }

    /// Parse format string and extract placeholders
    fn parse_format_placeholders(&self, format_str: &str) -> Vec<String> {
        let mut placeholders = Vec::new();
        let mut chars = format_str.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '{' {
                let mut placeholder = String::new();
                while let Some(&pc) = chars.peek() {
                    if pc == '}' {
                        chars.next();
                        break;
                    } else {
                        placeholder.push(chars.next().unwrap());
                    }
                }
                if !placeholder.is_empty() {
                    placeholders.push(placeholder);
                }
            }
        }

        placeholders
    }

    /// Check if type is u8 array or slice
    fn is_u8_array_or_slice(&self, ty: &Type) -> bool {
        match ty {
            Type::Array { element_type, .. } => {
                let inner = match element_type.as_ref() {
                    Type::Const(c) => c.as_ref(),
                    other => other,
                };
                matches!(inner, Type::U8)
            }
            _ => false,
        }
    }

    /// Check if type is integer type
    fn is_integer_type(&self, ty: &Type) -> bool {
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

    /// Check if type is float type
    fn is_float_type(&self, ty: &Type) -> bool {
        matches!(ty, Type::F32 | Type::F64)
    }

    /// Check if type is bool type
    fn is_bool_type(&self, ty: &Type) -> bool {
        matches!(ty, Type::Bool)
    }

    fn infer_int_literal_type(&self, value: i64, span: &Span) -> AnalysisResult<Type> {
        if let Some(expected_ty) = self.get_effective_expected_type() {
            // Handle Result types by extracting the inner (success) type
            let inner_ty = if expected_ty.is_result() {
                expected_ty.result_inner()
            } else {
                Some(&expected_ty)
            };

            if let Some(inner_ty) = inner_ty {
                if inner_ty.is_integer() {
                    if inner_ty.can_represent_int_literal(value) {
                        // Return the inner type, not the Result type
                        return Ok(inner_ty.clone());
                    }

                    return Err(AnalysisError::new_with_span(
                        &format!("Integer literal {} is out of range for {}", value, inner_ty),
                        span,
                    )
                    .with_module("infer"));
                }
                // If expected type is not integer but exists (e.g., float), try to convert
                if inner_ty.is_float() {
                    return Ok(inner_ty.clone());
                }
            }
        }

        // No expected type - default to i64 for integer literals
        Ok(Type::I64)
    }

    /// Infer type for float literal
    fn infer_float_literal_type(&self, value: f64, span: &Span) -> AnalysisResult<Type> {
        if let Some(expected_ty) = self.get_effective_expected_type() {
            if expected_ty.is_float() {
                // Check if value fits in f32 if expected type is f32
                if matches!(expected_ty, Type::F32) {
                    if value.is_nan() || value.is_infinite() {
                        return Ok(expected_ty.clone());
                    }
                    let abs_val = value.abs();
                    if abs_val > f32::MAX as f64
                        || (abs_val != 0.0 && abs_val < f32::MIN_POSITIVE as f64)
                    {
                        return Err(AnalysisError::new_with_span(
                            &format!("Float literal {} is out of range for f32", value),
                            span,
                        )
                        .with_module("infer"));
                    }
                }
                return Ok(expected_ty.clone());
            }
        }

        // No expected type - default to f64 for float literals
        Ok(Type::F64)
    }

    /// Check if two types are compatible for assignment-like contexts.
    fn types_compatible(&self, expected: &Type, actual: &Type) -> bool {
        if expected == actual {
            return true;
        }

        // Handle Const: const T is compatible with T
        // T is also compatible with const T (we can always promote to const)
        if let Type::Const(inner) = expected {
            if self.types_compatible(inner, actual) {
                return true;
            }
        }
        if let Type::Const(inner) = actual {
            if self.types_compatible(expected, inner) {
                return true;
            }
        }

        if let Type::Option(inner) = expected {
            if self.types_compatible(inner, actual) {
                return true;
            }
        }

        if let Type::Option(inner) = actual {
            if self.types_compatible(expected, inner) {
                return true;
            }
        }

        // Allow compatible numeric types
        if self.is_numeric(expected) && self.is_numeric(actual) {
            return true;
        }

        // Allow bool to numeric and numeric to bool conversions
        if matches!(expected, Type::Bool) && self.is_numeric(actual) {
            return true;
        }
        if self.is_numeric(expected) && matches!(actual, Type::Bool) {
            return true;
        }

        self.is_integer_type(expected) && self.is_integer_type(actual)
    }

    /// Infer types for an entire program
    pub fn infer_program(&mut self, program: &Program) -> AnalysisResult<TypedProgram> {
        // Populate structs, enums and errors maps from program
        for s in &program.structs {
            eprintln!(
                "DEBUG: inserting struct into self.structs: {:?}, generic_params={:?}",
                s.name, s.generic_params
            );
            self.structs.insert(s.name.clone(), s.clone());
        }
        for e in &program.enums {
            self.enums.insert(e.name.clone(), e.clone());
        }
        for e in &program.errors {
            self.errors.insert(e.name.clone(), e.clone());
        }
        for f in &program.functions {
            self.functions.insert(f.name.clone(), f.clone());
        }
        for s in &program.structs {
            for m in &s.methods {
                let full_name = format!("{}_{}", s.name, m.name);
                eprintln!(
                    "DEBUG: inserting struct method: {} (struct={}, method={}, struct_generic_params={:?}, method_generic_params={:?})",
                    full_name, s.name, m.name, s.generic_params, m.generic_params
                );
                // Combine struct's generic params with method's generic params
                let mut combined_params = s.generic_params.clone();
                combined_params.extend(m.generic_params.clone());

                let mut method_with_params = m.clone();
                method_with_params.generic_params = combined_params;
                method_with_params.name = full_name.clone();
                self.functions.insert(full_name, method_with_params);
            }
        }

        let mut functions = Vec::new();
        for f in &program.functions {
            if f.generic_params.is_empty() {
                functions.push(self.infer_fn(f)?);
            }
        }

        // Handle initial structs/enums/errors - only non-generic ones for now?
        // Actually, structs are types, we still need them in the TypedProgram.
        // First, collect struct methods that are in self.functions
        let mut structs = Vec::new();
        for s in &program.structs {
            if s.generic_params.is_empty() {
                // Add struct methods to initial functions (they were stored in self.functions)
                for m in &s.methods {
                    let full_name = format!("{}_{}", s.name, m.name);
                    if let Some(fn_def) = self.functions.get(&full_name) {
                        if fn_def.generic_params.is_empty() {
                            // Clone to avoid borrow conflict
                            let fn_def_clone = fn_def.clone();
                            if let Ok(typed_f) = self.infer_fn(&fn_def_clone) {
                                functions.push(typed_f);
                            }
                        }
                    }
                }
                structs.push(self.infer_struct(s)?);
            }
        }

        let mut enums = Vec::new();
        for e in &program.enums {
            // Add enum methods to initial functions
            for m in &e.methods {
                let full_name = format!("{}_{}", e.name, m.name);
                if let Some(fn_def) = self.functions.get(&full_name) {
                    if fn_def.generic_params.is_empty() {
                        // Clone to avoid borrow conflict
                        let mut fn_def_clone = fn_def.clone();
                        // Rename to include enum name prefix
                        fn_def_clone.name = full_name.clone();
                        match self.infer_fn(&fn_def_clone) {
                            Ok(mut typed_f) => {
                                // Also rename the typed function
                                typed_f.name = full_name.clone();
                                functions.push(typed_f);
                            }
                            Err(_) => {
                                // Skip methods that fail to infer
                            }
                        }
                    }
                }
            }
            enums.push(self.infer_enum(e)?);
        }

        let mut errors = Vec::new();
        for e in &program.errors {
            errors.push(self.infer_error(e)?);
        }

        // Finalize functions and structs including instantiations
        functions.extend(std::mem::take(&mut self.new_functions));
        eprintln!(
            "DEBUG final functions: {:?}",
            functions
                .iter()
                .map(|f| (&f.name, &f.return_ty))
                .collect::<Vec<_>>()
        );
        structs.extend(std::mem::take(&mut self.new_structs));
        eprintln!(
            "DEBUG final structs: {:?}",
            structs
                .iter()
                .map(|s| (&s.name, &s.generic_params, s.methods.len()))
                .collect::<Vec<_>>()
        );

        // Build ast_structs: all original structs (including generic) + monomorphized ones
        let mut ast_structs: Vec<crate::ast::StructDef> = program.structs.iter().cloned().collect();
        ast_structs.extend(self.new_ast_structs.clone());

        // Build typed interfaces
        let mut interfaces = Vec::new();
        for i in &program.interfaces {
            let mut methods = Vec::new();
            for m in &i.methods {
                methods.push(self.infer_fn(m)?);
            }
            interfaces.push(TypedInterfaceDef {
                name: i.name.clone(),
                methods,
                composed_interfaces: i.composed_interfaces.clone(),
                visibility: i.visibility,
                span: i.span,
            });
        }

        // Build ast_functions: original non-generic functions + monomorphized ones
        let mut ast_functions: Vec<crate::ast::FnDef> = program
            .functions
            .iter()
            .filter(|f| f.generic_params.is_empty())
            .cloned()
            .collect();
        let struct_method_names: HashSet<String> = ast_structs
            .iter()
            .flat_map(|s| {
                s.methods.iter().map(move |m| {
                    if m.name.starts_with(&format!("{}_", s.name)) {
                        m.name.clone()
                    } else {
                        format!("{}_{}", s.name, m.name)
                    }
                })
            })
            .collect();
        let new_ast_functions: Vec<crate::ast::FnDef> = self
            .new_ast_functions
            .clone()
            .into_iter()
            .filter(|f| !struct_method_names.contains(&f.name))
            .collect();
        ast_functions.extend(new_ast_functions);

        let mut external_functions = Vec::new();
        for ext_fn in &program.external_functions {
            external_functions.push(TypedFnDef {
                name: ext_fn.name.clone(),
                original_name: Some(ext_fn.name.clone()),
                visibility: ext_fn.visibility,
                params: ext_fn
                    .params
                    .iter()
                    .map(|p| TypedFnParam {
                        name: p.name.clone(),
                        ty: p.ty.clone(),
                    })
                    .collect(),
                return_ty: ext_fn.return_ty.clone(),
                body: Vec::new(),
                span: ext_fn.span,
            });
        }

        Ok(TypedProgram {
            functions,
            external_functions,
            structs,
            interfaces,
            enums,
            errors,
            imports: program.imports.clone(),
            ast_functions,
            ast_structs,
        })
    }

    /// Infer types for a struct definition
    fn infer_struct(&mut self, s: &crate::ast::StructDef) -> AnalysisResult<TypedStructDef> {
        let mut methods = Vec::new();
        for m in &s.methods {
            let mut tm = self.infer_fn(m)?;
            // Mangle name to include struct prefix
            tm.name = format!("{}_{}", s.name, m.name);
            methods.push(tm);
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
            let mut tm = self.infer_fn(m)?;
            // Mangle name to include enum prefix
            tm.name = format!("{}_{}", e.name, m.name);
            methods.push(tm);
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
            visibility: e.visibility,
            span: e.span,
        })
    }

    fn infer_type_args(
        &self,
        params: &[String],
        param_types: &[Type],
        arg_types: &[Type],
    ) -> AnalysisResult<Vec<Type>> {
        let mut mappings = HashMap::new();

        for (p_ty, a_ty) in param_types.iter().zip(arg_types.iter()) {
            self.match_type_params(p_ty, a_ty, &mut mappings);
        }

        let mut result = Vec::new();
        for p_name in params {
            if let Some(ty) = mappings.get(p_name) {
                result.push(ty.clone());
            } else {
                return Err(AnalysisError::new(&format!(
                    "Could not infer type argument for '{}'",
                    p_name
                ))
                .with_module("infer"));
            }
        }
        Ok(result)
    }

    fn match_type_params(&self, p_ty: &Type, a_ty: &Type, mappings: &mut HashMap<String, Type>) {
        match (p_ty, a_ty) {
            (Type::GenericParam(name), _) => {
                if !mappings.contains_key(name) {
                    mappings.insert(name.clone(), a_ty.clone());
                }
            }
            (Type::Pointer(p_inner), Type::Pointer(a_inner)) => {
                self.match_type_params(p_inner, a_inner, mappings);
            }
            (Type::Option(p_inner), Type::Option(a_inner)) => {
                self.match_type_params(p_inner, a_inner, mappings);
            }
            (Type::Result(p_inner), Type::Result(a_inner)) => {
                self.match_type_params(p_inner, a_inner, mappings);
            }
            (
                Type::Array {
                    element_type: p_elem,
                    ..
                },
                Type::Array {
                    element_type: a_elem,
                    ..
                },
            ) => {
                self.match_type_params(p_elem, a_elem, mappings);
            }
            (Type::Tuple(p_types), Type::Tuple(a_types)) if p_types.len() == a_types.len() => {
                for (pt, at) in p_types.iter().zip(a_types.iter()) {
                    self.match_type_params(pt, at, mappings);
                }
            }
            (
                Type::Custom {
                    name: p_name,
                    generic_args: p_args,
                    ..
                },
                Type::Custom {
                    name: a_name,
                    generic_args: a_args,
                    ..
                },
            ) if p_name == a_name && p_args.len() == a_args.len() => {
                for (pa, aa) in p_args.iter().zip(a_args.iter()) {
                    self.match_type_params(pa, aa, mappings);
                }
            }
            (
                Type::Function {
                    params: p_params,
                    return_type: p_ret,
                },
                Type::Function {
                    params: a_params,
                    return_type: a_ret,
                },
            ) if p_params.len() == a_params.len() => {
                for (pp, ap) in p_params.iter().zip(a_params.iter()) {
                    self.match_type_params(pp, ap, mappings);
                }
                self.match_type_params(p_ret, a_ret, mappings);
            }
            _ => {}
        }
    }

    fn get_mangled_name(&self, name: &str, type_args: &[Type]) -> String {
        let mut mangled = name.to_string();
        for ty in type_args {
            mangled.push('_');
            mangled.push_str(
                &ty.to_string()
                    .replace("<", "_")
                    .replace(">", "_")
                    .replace(",", "_")
                    .replace(" ", ""),
            );
        }
        mangled
    }

    fn instantiate_fn(
        &mut self,
        name: &str,
        type_args: &[Type],
        mangled_name: &str,
        is_struct_method: bool,
    ) -> AnalysisResult<()> {
        eprintln!(
            "DEBUG instantiate_fn: name={}, type_args={:?}, mangled_name={}, type_mappings_before={:?}",
            name, type_args, mangled_name, self.type_mappings
        );
        let f_def = self.functions.get(name).cloned().ok_or_else(|| {
            AnalysisError::new(&format!("Original function '{}' not found", name))
        })?;
        eprintln!(
            "DEBUG instantiate_fn: f_def.return_ty = {:?}",
            f_def.return_ty
        );

        let mut mappings = HashMap::new();
        for (i, p_name) in f_def.generic_params.iter().enumerate() {
            mappings.insert(p_name.clone(), type_args[i].clone());
        }
        eprintln!(
            "DEBUG instantiate_fn: f_def.generic_params={:?}, mappings={:?}",
            f_def.generic_params, mappings
        );

        // Save current type mappings
        let old_mappings = self.type_mappings.clone();
        self.type_mappings = mappings.clone();

        // Perform substitution on parameters and return type
        let mappings = self.type_mappings.clone();
        let instantiated_f = FnDef {
            name: mangled_name.to_string(),
            visibility: f_def.visibility,
            params: f_def
                .params
                .iter()
                .map(|p| FnParam {
                    name: p.name.clone(),
                    ty: self.substitute_type(&p.ty, &mappings),
                })
                .collect(),
            return_ty: self.substitute_type(&f_def.return_ty, &mappings),
            body: f_def.body.clone(),
            generic_params: Vec::new(),
            generic_constraints: Vec::new(),
            span: f_def.span,
        };

        // Store the AST-level FnDef for the lowering pass
        // Only for non-struct methods (they are handled via struct's methods)
        if !is_struct_method {
            self.new_ast_functions.push(instantiated_f.clone());
        }

        // Infer the instantiated function - keep the type mappings for inference
        // Note: We temporarily keep the mappings active for inference
        let mut typed_f = self.infer_fn(&instantiated_f)?;
        // Only add to global functions if not a struct method
        // Exception: struct constructors (new) should be in global functions
        if !is_struct_method {
            self.new_functions.push(typed_f);
        } else {
            let (owner_name, original_method_name) = name.split_once('_').ok_or_else(|| {
                AnalysisError::new(&format!(
                    "Struct method '{}' is missing its owner prefix",
                    name
                ))
            })?;
            let method_name = if let Some(pos) = mangled_name.rfind('_') {
                &mangled_name[pos + 1..]
            } else {
                original_method_name
            };
            if method_name == "new" {
                // Constructor - add to global functions
                self.new_functions.push(typed_f);
            } else {
                typed_f.original_name = Some(original_method_name.to_string());
                self.attach_instantiated_method_to_struct(
                    owner_name,
                    type_args,
                    instantiated_f,
                    typed_f,
                )?;
            }
        }

        // Restore type mappings
        self.type_mappings = old_mappings;
        Ok(())
    }

    fn attach_instantiated_method_to_struct(
        &mut self,
        owner_name: &str,
        type_args: &[Type],
        instantiated_ast: FnDef,
        instantiated_typed: TypedFnDef,
    ) -> AnalysisResult<()> {
        let owner_generic_len = self
            .structs
            .get(owner_name)
            .map(|s| s.generic_params.len())
            .ok_or_else(|| {
                AnalysisError::new(&format!(
                    "Original struct '{}' not found for method instantiation",
                    owner_name
                ))
            })?;

        let struct_type_args = &type_args[..owner_generic_len];
        let struct_mangled_name = self.get_mangled_name(owner_name, struct_type_args);

        if let Some(ast_struct) = self.structs.get_mut(&struct_mangled_name) {
            if !ast_struct
                .methods
                .iter()
                .any(|method| method.name == instantiated_ast.name)
            {
                ast_struct.methods.push(instantiated_ast.clone());
            }
        }

        if let Some(ast_struct) = self
            .new_ast_structs
            .iter_mut()
            .find(|struct_def| struct_def.name == struct_mangled_name)
        {
            if !ast_struct
                .methods
                .iter()
                .any(|method| method.name == instantiated_ast.name)
            {
                ast_struct.methods.push(instantiated_ast);
            }
        }

        if let Some(typed_struct) = self
            .new_structs
            .iter_mut()
            .find(|struct_def| struct_def.name == struct_mangled_name)
        {
            if !typed_struct
                .methods
                .iter()
                .any(|method| method.name == instantiated_typed.name)
            {
                typed_struct.methods.push(instantiated_typed);
            }
        }

        Ok(())
    }

    fn instantiate_struct(
        &mut self,
        name: &str,
        type_args: &[Type],
        mangled_name: &str,
    ) -> AnalysisResult<()> {
        eprintln!(
            "DEBUG instantiate_struct: name={}, type_args={:?}, mangled_name={}",
            name, type_args, mangled_name
        );
        let s_def =
            self.structs.get(name).cloned().ok_or_else(|| {
                AnalysisError::new(&format!("Original struct '{}' not found", name))
            })?;

        let mut mappings = HashMap::new();
        for (i, p_name) in s_def.generic_params.iter().enumerate() {
            mappings.insert(p_name.clone(), type_args[i].clone());
        }

        let old_mappings = self.type_mappings.clone();
        self.type_mappings = mappings;

        let mappings = self.type_mappings.clone();
        // For struct methods with additional generic params (like F in map<T, F>),
        // we need to create a mapping that substitutes only the struct's generic params
        // but leaves method's generic params as-is (they will be inferred during calls)
        // We use an empty mapping for method's params so they remain as GenericParam
        let instantiated_methods: Vec<FnDef> = s_def
            .methods
            .iter()
            .filter(|m| m.generic_params.is_empty())
            .map(|m| {
                let method_name = format!("{}_{}", s_def.name, m.name);
                // For method's params, use only struct's mappings (not method's extra generic params)
                // Method's extra generic params will remain as GenericParam in the signature
                // and will be inferred when the method is actually called
                FnDef {
                    name: method_name,
                    visibility: m.visibility,
                    params: m
                        .params
                        .iter()
                        .map(|p| FnParam {
                            name: p.name.clone(),
                            ty: self.substitute_type(&p.ty, &mappings),
                        })
                        .collect(),
                    return_ty: self.substitute_type(&m.return_ty, &mappings),
                    body: m.body.clone(),
                    generic_params: Vec::new(),
                    generic_constraints: Vec::new(),
                    span: m.span,
                }
            })
            .collect();
        let instantiated_s = crate::ast::StructDef {
            name: mangled_name.to_string(),
            fields: s_def
                .fields
                .iter()
                .map(|f| crate::ast::StructField {
                    name: f.name.clone(),
                    ty: self.substitute_type(&f.ty, &mappings),
                    visibility: f.visibility,
                })
                .collect(),
            methods: instantiated_methods,
            interface_impls: s_def.interface_impls.clone(),
            visibility: s_def.visibility,
            generic_params: Vec::new(),
            span: s_def.span,
        };

        // Store the AST-level StructDef for the lowering pass
        self.new_ast_structs.push(instantiated_s.clone());
        self.structs
            .insert(mangled_name.to_string(), instantiated_s.clone());

        let typed_s = self.infer_struct(&instantiated_s);
        if let Ok(ts) = typed_s {
            self.new_structs.push(ts);
        }

        self.type_mappings = old_mappings;
        Ok(())
    }

    /// Infer types for a function definition
    fn infer_fn(&mut self, f: &FnDef) -> AnalysisResult<TypedFnDef> {
        // Enter function scope and add parameters
        self.symbol_table.enter_scope();
        let previous_generic_constraints = self.current_generic_constraints.clone();
        self.current_generic_constraints.clear();
        for (param, interface) in &f.generic_constraints {
            self.current_generic_constraints
                .entry(param.clone())
                .or_default()
                .push(interface.clone());
        }

        let mappings = self.type_mappings.clone();
        eprintln!(
            "DEBUG infer_fn: f.name={}, type_mappings={:?}, f.return_ty={:?}",
            f.name, mappings, f.return_ty
        );
        let mut typed_params = Vec::new();
        for param in &f.params {
            let substituted_ty = self.substitute_type(&param.ty, &mappings);
            self.symbol_table.define(
                param.name.clone(),
                substituted_ty.clone(),
                Visibility::Private,
                false,
            );
            typed_params.push(TypedFnParam {
                name: param.name.clone(),
                ty: substituted_ty,
            });
        }

        // Set expected return type for return statements
        let previous_return_type = self.expected_return_type.clone();
        let substituted_return_ty = self.substitute_type(&f.return_ty, &mappings);
        self.expected_return_type = Some(substituted_return_ty.clone());

        // Save and reset try expression tracking
        let previous_has_try = self.has_try_expression;
        let previous_try_span = self.try_expression_span;
        self.has_try_expression = false;
        self.try_expression_span = None;

        // Infer types for the function body
        let mut body = Vec::new();
        for stmt in &f.body {
            let typed_stmt = self.infer_stmt(stmt)?;
            body.push(typed_stmt);
        }

        // Check: if function body contains try expression, return type must be Result
        if self.has_try_expression {
            if !substituted_return_ty.is_result() {
                let span = self.try_expression_span.unwrap_or(f.span);
                return Err(AnalysisError::new_with_span(
                    &format!(
                        "Function '{}' contains try expression but does not return a Result type. Try expressions require the function to return a Result type to propagate errors.",
                        f.name
                    ),
                    &span,
                ).with_module("infer"));
            }
        }

        // Restore try expression tracking
        self.has_try_expression = previous_has_try;
        self.try_expression_span = previous_try_span;

        // Restore previous return type
        self.expected_return_type = previous_return_type;
        self.current_generic_constraints = previous_generic_constraints;

        self.symbol_table.exit_scope();

        Ok(TypedFnDef {
            name: f.name.clone(),
            original_name: None,
            visibility: f.visibility,
            params: typed_params,
            return_ty: substituted_return_ty,
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
            Stmt::Continue { span, .. } => *span,
        };

        match stmt {
            Stmt::Expr { expr, span } => {
                let typed_expr = self.infer_expr(expr)?;

                // Check if this is a Try expression used as a statement
                // If so, the return value must be consumed (unless it's void)
                if let Expr::Try {
                    expr: _try_expr,
                    span: try_span,
                } = expr
                {
                    // Get the type of the inner expression before Try unwrapping
                    // The Try expression type is already unwrapped, so we need to check
                    // if the original function returns a Result type
                    let inner_ty = if let Type::Result(inner) = &typed_expr.ty {
                        inner.as_ref().clone()
                    } else {
                        typed_expr.ty.clone()
                    };

                    // If the result type is not void, it must be consumed
                    if inner_ty != Type::Void {
                        return Err(AnalysisError::new_with_span(
                            "Try expression returns a value that must be consumed. Use 'const x = try ...' or '_ = try ...' to consume the result.",
                            try_span,
                        ).with_module("infer"));
                    }
                }

                Ok(TypedStmt {
                    stmt: TypedStmtKind::Expr { expr: typed_expr },
                    span: *span,
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
                    self.infer_expr(val_expr)?.ty.clone()
                } else {
                    return Err(AnalysisError::new_with_span(
                        "Variable must have either a type or an initial value",
                        span,
                    )
                    .with_module("infer"));
                };

                // Define the variable in the symbol table
                if let Some(ns) = names {
                    for (index, name_opt) in ns.iter().enumerate() {
                        if let Some(n) = name_opt {
                            self.symbol_table.define(
                                n.clone(),
                                destructured_binding_type_checked(&inferred_ty, index, span)?,
                                Visibility::Private,
                                matches!(mutability, crate::ast::Mutability::Const),
                            );
                        }
                    }
                } else {
                    // For single declaration, we might want to store the value if it's a const.
                    // However, the value is not inferred yet. We'll define it with None first
                    // and then update it if it's a const.
                    self.symbol_table.define(
                        name.clone(),
                        inferred_ty.clone(),
                        Visibility::Private,
                        matches!(mutability, crate::ast::Mutability::Const),
                    );
                }

                // Set expected_type before inferring value if explicit type is provided
                if let Some(explicit_ty) = ty {
                    self.set_expected_type(Some(explicit_ty.clone()));
                }

                let typed_value = if let Some(val_expr) = value {
                    let result = self.infer_expr(val_expr)?;
                    // Clear expected_type after inference
                    self.set_expected_type(None);

                    // If it's a constant, update the symbol table with the value
                    if matches!(mutability, crate::ast::Mutability::Const) {
                        if let Some(const_val) = self.extract_constant_value(&result) {
                            if let Some(symbol) = self.symbol_table.resolve_mut(name) {
                                symbol.const_value = Some(const_val);
                            }
                        }
                    }
                    Some(result)
                } else {
                    None
                };

                if let Some(explicit_ty) = ty {
                    if let Some(typed_value) = &typed_value {
                        if !self.types_compatible(explicit_ty, &typed_value.ty) {
                            return Err(AnalysisError::new_with_span(
                                &format!(
                                    "Type mismatch in declaration '{}': expected {}, found {}",
                                    name, explicit_ty, typed_value.ty
                                ),
                                span,
                            )
                            .with_module("infer"));
                        }
                    }
                }

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
                op,
                span,
            } => {
                let typed_value = self.infer_expr(value)?;
                Ok(TypedStmt {
                    stmt: TypedStmtKind::Assign {
                        target: target.clone(),
                        op: *op,
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
                let binding_ty = for_binding_type(iterable, &typed_iterable.ty)?;
                let index_ty = for_index_type(iterable, &typed_iterable.ty)?;

                if (var_name.is_some() || capture.is_some()) && binding_ty.is_none() {
                    let message = if matches!(iterable, Expr::Null(_)) {
                        "Infinite for loop cannot bind loop variables".to_string()
                    } else {
                        format!(
                            "Cannot infer loop variable type from iterable of type {}",
                            typed_iterable.ty
                        )
                    };
                    return Err(AnalysisError::new_with_span(&message, span).with_module("infer"));
                }

                if index_var.is_some() && index_ty.is_none() {
                    return Err(AnalysisError::new_with_span(
                        "For loop index variable is only supported when iterating over arrays",
                        span,
                    )
                    .with_module("infer"));
                }

                self.symbol_table.enter_scope();

                if let Some(vn) = var_name {
                    self.symbol_table.define(
                        vn.clone(),
                        binding_ty.clone().expect("binding type checked above"),
                        Visibility::Private,
                        false,
                    );
                }
                if let Some(cv) = capture {
                    self.symbol_table.define(
                        cv.clone(),
                        binding_ty.clone().expect("binding type checked above"),
                        Visibility::Private,
                        false,
                    );
                }
                if let Some(iv) = index_var {
                    self.symbol_table.define(
                        iv.clone(),
                        index_ty.expect("index type checked above"),
                        Visibility::Private,
                        false,
                    );
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
            Stmt::Continue { label, span } => Ok(TypedStmt {
                stmt: TypedStmtKind::Continue {
                    label: label.clone(),
                },
                span: *span,
            }),
        }
    }

    /// Infer the type of an expression
    fn infer_expr(&mut self, expr: &Expr) -> AnalysisResult<TypedExpr> {
        eprintln!("DEBUG: infer_expr called");
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
            Expr::Index { span, .. } => *span,
            Expr::Ident(_, span) => *span,
            Expr::Array(_, _, span) => *span,
            Expr::Binary { span, .. } => *span,
            Expr::Unary { span, .. } => *span,
            Expr::Call { span, .. } => *span,
            Expr::If { span, .. } => *span,
            Expr::Block { span, .. } => *span,
            Expr::MemberAccess { span, .. } => *span,
            Expr::Struct { span, .. } => *span,
            Expr::Try { span, .. } => *span,
            Expr::Catch { span, .. } => *span,
            Expr::Cast { span, .. } => *span,
            Expr::Dereference { span, .. } => *span,
            Expr::Intrinsic { span, .. } => *span,
            Expr::TypeLiteral(_, span) => *span,
        };

        match expr {
            Expr::Int(value, _) => Ok(TypedExpr {
                expr: TypedExprKind::Int(*value),
                ty: self.infer_int_literal_type(*value, &span)?,
                span,
            }),
            Expr::Float(value, _) => Ok(TypedExpr {
                expr: TypedExprKind::Float(*value),
                ty: self.infer_float_literal_type(*value, &span)?,
                span,
            }),
            Expr::Bool(value, _) => Ok(TypedExpr {
                expr: TypedExprKind::Bool(*value),
                ty: Type::Bool,
                span,
            }),
            Expr::String(value, _) => Ok(TypedExpr {
                expr: TypedExprKind::String(value.clone()),
                ty: Type::Array {
                    size: None,
                    element_type: Box::new(Type::Const(Box::new(Type::U8))),
                },
                span,
            }),
            Expr::Char(value, _) => {
                // Use expected type if available, otherwise default to i8
                let ty = self.get_expected_type().cloned().unwrap_or(Type::I8);
                Ok(TypedExpr {
                    expr: TypedExprKind::Char(*value),
                    ty,
                    span,
                })
            }
            Expr::Null(span) => {
                // Use expected type if available (to support rawptr, optional, etc.),
                // otherwise default to Option<i64>
                let ty = match self.get_effective_expected_type() {
                    Some(Type::RawPtr) => Type::RawPtr,
                    Some(Type::Pointer(inner)) => Type::Pointer(inner.clone()),
                    Some(Type::Option(inner)) => Type::Option(inner.clone()),
                    Some(expected) => {
                        return Err(AnalysisError::new_with_span(
                            &format!("Type mismatch: cannot assign null to {}", expected),
                            span,
                        )
                        .with_module("infer"));
                    }
                    None => Type::Option(Box::new(Type::I64)),
                };
                Ok(TypedExpr {
                    expr: TypedExprKind::Null,
                    ty,
                    span: *span,
                })
            }
            Expr::Tuple(elements, _) => {
                // Get expected tuple type from context if provided
                let expected_tuple_type =
                    self.get_expected_type()
                        .and_then(|expected_ty| match expected_ty {
                            Type::Tuple(types) => {
                                eprintln!(
                                    "DEBUG Tuple: expected_type is Some(Tuple), types={:?}",
                                    types
                                );
                                Some(types.clone())
                            }
                            _ => {
                                eprintln!("DEBUG Tuple: expected_type is Some({:?})", expected_ty);
                                None
                            }
                        });

                let mut typed_elements = Vec::new();
                let previous_expected_type = self.get_expected_type().cloned();

                // Validation: if we have expected tuple types, check length
                if let Some(ref expected_types) = expected_tuple_type {
                    if elements.len() != expected_types.len() {
                        return Err(AnalysisError::new_with_span(
                            &format!(
                                "Tuple literal expected {} elements, found {}",
                                expected_types.len(),
                                elements.len()
                            ),
                            &span,
                        )
                        .with_module("infer"));
                    }
                }

                for (index, elem) in elements.iter().enumerate() {
                    // Set expected type for each element if we have an expected tuple type
                    if let Some(ref expected_types) = expected_tuple_type {
                        if index < expected_types.len() {
                            self.set_expected_type(Some(expected_types[index].clone()));
                        }
                    }
                    typed_elements.push(self.infer_expr(elem)?);
                    self.set_expected_type(previous_expected_type.clone());
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
            Expr::Index {
                object,
                index,
                span,
            } => {
                let typed_object = self.infer_expr(object)?;
                let typed_index = self.infer_expr(index)?;

                let element_type = match &typed_object.ty {
                    Type::Array { element_type, .. } => element_type.clone(),
                    _ => {
                        return Err(AnalysisError::new_with_span(
                            &format!(
                                "Indexing only supported on array or slice types, found {}",
                                typed_object.ty
                            ),
                            &typed_object.span,
                        )
                        .with_module("infer"));
                    }
                };

                // Check if it's a range access (slicing)
                if typed_index.ty.is_integer() {
                    // Single element access: returns T
                    Ok(TypedExpr {
                        expr: TypedExprKind::Index {
                            object: Box::new(typed_object),
                            index: Box::new(typed_index),
                        },
                        ty: *element_type,
                        span: *span,
                    })
                } else if let TypedExprKind::Binary {
                    op: BinaryOp::Range,
                    ..
                } = &typed_index.expr
                {
                    // Slice access: returns []T
                    Ok(TypedExpr {
                        expr: TypedExprKind::Index {
                            object: Box::new(typed_object),
                            index: Box::new(typed_index),
                        },
                        ty: Type::Array {
                            size: None,
                            element_type,
                        },
                        span: *span,
                    })
                } else {
                    Err(AnalysisError::new_with_span(
                        "Array index must be an integer or a range",
                        &typed_index.span,
                    )
                    .with_module("infer"))
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
            Expr::Array(elements, explicit_ty, span) => {
                // Get expected element type from explicit type in AST if provided
                let explicit_element_type = explicit_ty.clone();

                // Get expected element type from expected_type (context) if provided
                let context_element_type = if let Some(expected_ty) = self.get_expected_type() {
                    // Check array size if expected_ty is fixed-size array
                    if let Type::Array {
                        size: Some(expected_size),
                        ..
                    } = expected_ty
                    {
                        if *expected_size != elements.len() {
                            return Err(AnalysisError::new_with_span(
                                &format!(
                                    "Array literal expected {} elements, found {}",
                                    expected_size,
                                    elements.len()
                                ),
                                &span,
                            )
                            .with_module("infer"));
                        }
                    }

                    if let Type::Array { element_type, .. } = expected_ty {
                        Some(*element_type.clone())
                    } else {
                        None
                    }
                } else {
                    None
                };

                // Prefer explicit type from AST, then context type
                let expected_element_type = explicit_element_type.or(context_element_type);

                let previous_expected_type = self.get_expected_type().cloned();
                let mut typed_elements = Vec::new();
                for (index, elem) in elements.iter().enumerate() {
                    // Set expected type for each element if we have an expected element type
                    if let Some(ref elem_ty) = expected_element_type {
                        self.set_expected_type(Some(elem_ty.clone()));
                    }
                    let typed_elem = self.infer_expr(elem)?;
                    self.set_expected_type(previous_expected_type.clone());

                    if let Some(ref elem_ty) = expected_element_type {
                        if !self.types_compatible(elem_ty, &typed_elem.ty) {
                            return Err(AnalysisError::new_with_span(
                                &format!(
                                    "Array element #{} has type {}, expected {}",
                                    index + 1,
                                    typed_elem.ty,
                                    elem_ty
                                ),
                                &typed_elem.span,
                            )
                            .with_module("infer"));
                        }
                    } else if let Some(first_ty) =
                        typed_elements.first().map(|e: &TypedExpr| e.ty.clone())
                    {
                        if !self.types_compatible(&first_ty, &typed_elem.ty) {
                            return Err(AnalysisError::new_with_span(
                                &format!(
                                    "Array element #{} has type {}, expected {}",
                                    index + 1,
                                    typed_elem.ty,
                                    first_ty
                                ),
                                &typed_elem.span,
                            )
                            .with_module("infer"));
                        }
                    }

                    typed_elements.push(typed_elem);
                }

                self.set_expected_type(previous_expected_type);

                // Determine the element type: prefer explicit type, then context type, otherwise use first element's type
                let element_type = if let Some(elem_ty) = expected_element_type {
                    elem_ty
                } else if typed_elements.is_empty() {
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
                    span: *span,
                })
            }
            Expr::Binary {
                op,
                left,
                right,
                span,
            } => {
                // For binary operations, we need to infer the left operand first
                // If there's no expected type from context, use i64 as default for the first operand
                let previous_expected = self.get_expected_type().cloned();

                if previous_expected.is_none() {
                    self.set_expected_type(Some(Type::I64));
                }

                // First infer left operand
                let typed_left = self.infer_expr(left)?;

                // Use left operand's type as expected type for right operand
                self.set_expected_type(Some(typed_left.ty.clone()));

                let typed_right = self.infer_expr(right)?;

                // Restore previous expected type
                self.set_expected_type(previous_expected);

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
                generic_args,
                span,
            } => {
                // Pre-infer argument types as they are needed for built-ins and resolution
                let mut typed_args = Vec::new();
                for arg in args {
                    typed_args.push(self.infer_expr(arg)?);
                }

                // 1. Handle Built-in functions
                if namespace.is_none() {
                    match name.as_str() {
                        "println" => {
                            // Validation (Special handling for io prefix might be needed if moved from io::println)
                            // But usually it's io::println. Let's keep existing io::println check below.
                        }
                        _ => {}
                    }
                }

                if namespace.as_deref() == Some("io") && name == "println" {
                    if let Some(first_arg) = args.first() {
                        if let Expr::String(format_str, _) = first_arg {
                            self.validate_io_println_format(format_str, &typed_args[1..], span)?;
                        }
                    }
                    return Ok(TypedExpr {
                        expr: TypedExprKind::Call {
                            name: name.clone(),
                            namespace: namespace.clone(),
                            args: typed_args,
                            target_ty: None,
                        },
                        ty: Type::Void,
                        span: *span,
                    });
                }

                // 2. Resolve as a normal function (with Generics)
                let mut fn_lookup_name = if let Some(ns) = namespace {
                    format!("{}_{}", ns, name)
                } else {
                    name.clone()
                };

                let mut instance_type = None;
                let mut effective_namespace = namespace.clone();
                let mut effective_generic_args = generic_args.clone();

                if let Some(ns) = namespace {
                    eprintln!("DEBUG resolve_call - ns={}, name={}", ns, name);
                    if let Some(var_symbol) = self.symbol_table.resolve(ns) {
                        eprintln!("DEBUG resolve_call - var_symbol.ty={:?}", var_symbol.ty);
                        if let Some(struct_full_name) = custom_type_name(&var_symbol.ty) {
                            eprintln!("DEBUG resolve_call - struct_full_name={}", struct_full_name);

                            // Check if this is a type reference (generic struct name) or variable reference (monomorphized)
                            // If struct_full_name == ns, it's a type reference (static call like Compose.new)
                            // If struct_full_name != ns, it's a variable reference (instance call like comp.map)
                            let is_type_reference = ns == struct_full_name;

                            // Try to reverse-map a monomorphized name back to original + type_args
                            // Only reverse-map if struct_full_name is actually a mangled name (not the original)
                            let reverse = self
                                .struct_instantiations
                                .iter()
                                .find(|(_, v)| v.as_str() == struct_full_name)
                                .map(|((orig, args), _)| (orig.clone(), args.clone()));

                            eprintln!(
                                "DEBUG resolve_call - reverse={:?}, is_type_reference={}",
                                reverse, is_type_reference
                            );

                            if let Some((original_name, struct_args)) = reverse {
                                fn_lookup_name = format!("{}_{}", original_name, name);
                                let mut combined_args = struct_args;
                                combined_args.extend(generic_args.clone());
                                effective_generic_args = combined_args;
                                effective_namespace = Some(original_name);
                                // Only set instance_type for actual variable references, not type references
                                if !is_type_reference {
                                    instance_type = Some(var_symbol.ty.clone());
                                }
                            } else if !is_type_reference {
                                // Variable reference but no reverse mapping - use the full name directly
                                fn_lookup_name = format!("{}_{}", struct_full_name, name);
                                effective_namespace = Some(struct_full_name.to_string());
                                instance_type = Some(var_symbol.ty.clone());
                            } else {
                                // Type reference - no reverse mapping needed
                                fn_lookup_name = format!("{}_{}", struct_full_name, name);
                                effective_namespace = Some(struct_full_name.to_string());
                            }
                        }
                    }
                }

                eprintln!("DEBUG resolve_call - fn_lookup_name={}", fn_lookup_name);
                if let Some(symbol) = self.symbol_table.resolve(&fn_lookup_name).cloned() {
                    eprintln!("DEBUG resolve_call - found symbol: {:?}", symbol.ty);
                    let (actual_generic_args, mangled_name, method_mangled) = if !symbol
                        .generic_params
                        .is_empty()
                    {
                        let mut type_args = effective_generic_args;
                        if type_args.len() < symbol.generic_params.len() {
                            if let Type::Function { params, .. } = &symbol.ty {
                                // Build a pre-seeded mapping from already-known type args
                                // (e.g. struct's T=I32) so that when we match remaining params
                                // the known types act as constraints.
                                let mut pre_seeded: HashMap<String, Type> = HashMap::new();
                                for (i, p_name) in symbol.generic_params.iter().enumerate() {
                                    if i < type_args.len() {
                                        pre_seeded.insert(p_name.clone(), type_args[i].clone());
                                    }
                                }

                                let inferred_arg_types: Vec<Type> =
                                    typed_args.iter().map(|e| e.ty.clone()).collect();

                                // Try to infer remaining params from call arguments (no instance prepend
                                // since we're matching against the non-self params of the function).
                                // Use only the call args (not instance) for inferring method-specific params.
                                let mut mappings = pre_seeded;
                                for (p_ty, a_ty) in params
                                    .iter()
                                    .skip(if instance_type.is_some() { 1 } else { 0 })
                                    .zip(inferred_arg_types.iter())
                                {
                                    self.match_type_params(p_ty, a_ty, &mut mappings);
                                }

                                // Fill in type_args for remaining (un-filled) params
                                for i in type_args.len()..symbol.generic_params.len() {
                                    let p_name = &symbol.generic_params[i];
                                    if let Some(ty) = mappings.get(p_name) {
                                        type_args.push(ty.clone());
                                    } else {
                                        return Err(AnalysisError::new_with_span(
                                            &format!(
                                                "Could not infer type argument for '{}' in '{}'",
                                                p_name, fn_lookup_name
                                            ),
                                            span,
                                        )
                                        .with_module("infer"));
                                    }
                                }
                            }
                        }

                        if type_args.len() != symbol.generic_params.len() {
                            return Err(AnalysisError::new_with_span(
                                &format!("Generic function '{}' expects {} type arguments, but {} were provided/inferred", 
                                    fn_lookup_name, symbol.generic_params.len(), type_args.len()),
                                span,
                            ).with_module("infer"));
                        }

                        let key = (fn_lookup_name.clone(), type_args.clone());
                        let (final_args, mangled, method_mangled) = if let Some(mangled) =
                            self.fn_instantiations.get(&key)
                        {
                            // Derive method_mangled from cached mangled by stripping struct prefix
                            let m_opt = if let Some(ns) = &effective_namespace {
                                if let Some(s_def) = self.structs.get(ns) {
                                    let s_len = s_def.generic_params.len();
                                    let s_mangled = self.get_mangled_name(ns, &type_args[..s_len]);
                                    mangled
                                        .strip_prefix(&format!("{}_", s_mangled))
                                        .map(|s| s.to_string())
                                } else {
                                    None
                                }
                            } else {
                                None
                            };
                            (type_args.clone(), mangled.clone(), m_opt)
                        } else {
                            let (mangled, m_opt) = if let Some(ns) = &effective_namespace {
                                if let Some(s_def) = self.structs.get(ns) {
                                    let s_len = s_def.generic_params.len();
                                    let s_mangled = self.get_mangled_name(ns, &type_args[..s_len]);
                                    if s_len < type_args.len() {
                                        let m = self.get_mangled_name(name, &type_args[s_len..]);
                                        (format!("{}_{}", s_mangled, m), Some(m))
                                    } else {
                                        (format!("{}_{}", s_mangled, name), Some(name.clone()))
                                    }
                                } else if let Some(e_def) = self.enums.get(ns) {
                                    let e_len = e_def.generic_params.len();
                                    let e_mangled = self.get_mangled_name(ns, &type_args[..e_len]);
                                    if e_len < type_args.len() {
                                        let m = self.get_mangled_name(name, &type_args[e_len..]);
                                        (format!("{}_{}", e_mangled, m), Some(m))
                                    } else {
                                        (format!("{}_{}", e_mangled, name), Some(name.clone()))
                                    }
                                } else {
                                    (self.get_mangled_name(&fn_lookup_name, &type_args), None)
                                }
                            } else {
                                (self.get_mangled_name(&fn_lookup_name, &type_args), None)
                            };
                            self.fn_instantiations.insert(key.clone(), mangled.clone());
                            // instantiate_fn pushes to new_functions internally; return value is ()
                            // Pass effective_namespace.is_some() to indicate if this is a struct method
                            self.instantiate_fn(
                                &fn_lookup_name,
                                &type_args,
                                &mangled,
                                effective_namespace.is_some(),
                            )?;
                            (type_args, mangled, m_opt)
                        };

                        (final_args, mangled, method_mangled)
                    } else {
                        (Vec::new(), fn_lookup_name.clone(), None)
                    };

                    let final_symbol = if mangled_name != fn_lookup_name {
                        let mut mappings = HashMap::new();
                        for (i, p_name) in symbol.generic_params.iter().enumerate() {
                            mappings.insert(p_name.clone(), actual_generic_args[i].clone());
                        }
                        let substituted_ty = self.substitute_type(&symbol.ty, &mappings);
                        Symbol {
                            name: mangled_name.clone(),
                            ty: substituted_ty,
                            visibility: symbol.visibility,
                            is_const: symbol.is_const,
                            const_value: None,
                            generic_params: Vec::new(),
                        }
                    } else {
                        symbol
                    };

                    let return_type = if let Type::Function { return_type, .. } = &final_symbol.ty {
                        return_type.as_ref().clone()
                    } else {
                        return Err(AnalysisError::new_with_span(
                            &format!("'{}' is not a function", fn_lookup_name),
                            span,
                        )
                        .with_module("infer"));
                    };

                    // For instance method calls, emit:
                    //   name = method-only mangled (e.g., "map_i32")
                    //   namespace = original variable (e.g., "comp")
                    // Codegen reconstructs the full name as "Compose_i32_map_i32".
                    // For static generic calls, suppress namespace and use the full mangled name.
                    // Check if this is a type reference (generic struct name) or variable reference (monomorphized)
                    // If namespace == effective_namespace, it's a type reference (static call like Compose.new)
                    // If namespace != effective_namespace, it's a variable reference (instance call like comp.map)
                    let is_static_call = namespace.as_deref() == effective_namespace.as_deref();

                    let (emit_name, emit_namespace) = if instance_type.is_some() && !is_static_call
                    {
                        // Instance method call: name = "map_i32", namespace = variable name (e.g., "comp")
                        let meth_name = method_mangled.unwrap_or_else(|| name.clone());
                        (meth_name, namespace.clone())
                    } else if mangled_name != fn_lookup_name {
                        // Static generic call: use full mangled name (e.g., "Compose_i32_new") with namespace=None
                        (mangled_name.clone(), None)
                    } else {
                        (name.clone(), effective_namespace)
                    };

                    eprintln!(
                        "DEBUG resolve_call - emit_name={}, emit_namespace={:?}, mangled_name={}, is_static_call={}",
                        emit_name, emit_namespace, mangled_name, is_static_call
                    );

                    return Ok(TypedExpr {
                        expr: TypedExprKind::Call {
                            name: emit_name,
                            namespace: emit_namespace,
                            args: typed_args,
                            target_ty: instance_type.clone(),
                        },
                        ty: return_type,
                        span: *span,
                    });
                }

                // 3. Resolve as a method call or field function
                if let Some(ns) = namespace {
                    if let Some(var_symbol) = self.symbol_table.resolve(ns) {
                        if let Type::GenericParam(param_name) = &var_symbol.ty {
                            if let Some(interface_names) =
                                self.current_generic_constraints.get(param_name)
                            {
                                for interface_name in interface_names {
                                    if let Some(interface_def) = self.interfaces.get(interface_name)
                                    {
                                        if let Some(method) =
                                            interface_def.methods.iter().find(|m| &m.name == name)
                                        {
                                            return Ok(TypedExpr {
                                                expr: TypedExprKind::Call {
                                                    name: name.clone(),
                                                    namespace: namespace.clone(),
                                                    args: typed_args,
                                                    target_ty: Some(var_symbol.ty.clone()),
                                                },
                                                ty: method.return_ty.clone(),
                                                span: *span,
                                            });
                                        }
                                    }
                                }
                            }
                        }

                        if let Some(struct_full_name) = custom_type_name(&var_symbol.ty) {
                            if let Some(struct_def) = self.structs.get(struct_full_name) {
                                // 1. Check for methods
                                if let Some(method) =
                                    struct_def.methods.iter().find(|m| &m.name == name)
                                {
                                    return Ok(TypedExpr {
                                        expr: TypedExprKind::Call {
                                            name: name.clone(),
                                            namespace: namespace.clone(),
                                            args: typed_args,
                                            target_ty: Some(var_symbol.ty.clone()),
                                        },
                                        ty: method.return_ty.clone(),
                                        span: *span,
                                    });
                                }

                                // 2. Check for fields that are function types
                                if let Some(field) =
                                    struct_def.fields.iter().find(|f| &f.name == name)
                                {
                                    if let Type::Function { return_type, .. } = &field.ty {
                                        return Ok(TypedExpr {
                                            expr: TypedExprKind::Call {
                                                name: name.clone(),
                                                namespace: namespace.clone(),
                                                args: typed_args,
                                                target_ty: Some(var_symbol.ty.clone()),
                                            },
                                            ty: return_type.as_ref().clone(),
                                            span: *span,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }

                Err(AnalysisError::new_with_span(
                    &format!("Undefined function '{}'", fn_lookup_name),
                    span,
                )
                .with_module("infer"))
            }
            Expr::Intrinsic { name, args, span } => {
                let intrinsic =
                    crate::sema::intrinsics::Intrinsic::from_name(name).ok_or_else(|| {
                        AnalysisError::new_with_span(
                            &format!("Unknown intrinsic function '{}'", name),
                            span,
                        )
                        .with_module("infer")
                    })?;

                // Pre-infer argument types
                let mut typed_args = Vec::new();
                for arg in args {
                    typed_args.push(self.infer_expr(arg)?);
                }

                // Initial validation of arguments count
                intrinsic.validate_args(args, *span)?;

                // Further validation based on inferred types
                let arg_types: Vec<Type> = typed_args.iter().map(|a| a.ty.clone()).collect();
                intrinsic.validate_types(&arg_types, *span)?;

                // For SizeOf/AlignOf, current inferrer expects first arg to be TypeLiteral
                if matches!(
                    intrinsic,
                    crate::sema::intrinsics::Intrinsic::SizeOf
                        | crate::sema::intrinsics::Intrinsic::AlignOf
                ) {
                    if !matches!(typed_args[0].expr, TypedExprKind::TypeLiteral(_)) {
                        return Err(AnalysisError::new_with_span(
                            &format!("{} requires a type argument", name),
                            span,
                        )
                        .with_module("infer"));
                    }
                }

                Ok(TypedExpr {
                    expr: TypedExprKind::Intrinsic {
                        name: name.clone(),
                        args: typed_args.clone(),
                    },
                    ty: intrinsic.return_type_from_args(&typed_args),
                    span: *span,
                })
            }
            Expr::TypeLiteral(ty, _) => Ok(TypedExpr {
                expr: TypedExprKind::TypeLiteral(ty.clone()),
                ty: Type::Void, // Type literals don't have a value type in the language
                span,
            }),
            Expr::Struct {
                name,
                fields,
                generic_args,
                span,
            } => {
                let s_def = self.structs.get(name).cloned().ok_or_else(|| {
                    AnalysisError::new_with_span(&format!("Undefined struct '{}'", name), span)
                        .with_module("infer")
                })?;

                let (actual_generic_args, mangled_name) = if !s_def.generic_params.is_empty() {
                    let type_args = if generic_args.is_empty() {
                        return Err(AnalysisError::new_with_span(
                            &format!("Generic arguments required for struct '{}'", name),
                            span,
                        )
                        .with_module("infer"));
                    } else {
                        generic_args.clone()
                    };

                    // Substitute type args using current mappings
                    let substituted_type_args: Vec<Type> = type_args
                        .iter()
                        .map(|t| self.substitute_type(t, &HashMap::new()))
                        .collect();

                    // Skip instantiation if any type_arg is generic - these will be handled during instantiation
                    let any_generic = substituted_type_args.iter().any(|t| t.is_generic());

                    if any_generic {
                        // Don't instantiate - just return the original name
                        (Vec::<Type>::new(), name.clone())
                    } else {
                        let key = (name.clone(), substituted_type_args.clone());
                        if let Some(mangled) = self.struct_instantiations.get(&key) {
                            // Monomorphized struct - no generic args needed
                            (Vec::<Type>::new(), mangled.clone())
                        } else {
                            let mangled = self.get_mangled_name(name, &substituted_type_args);
                            self.struct_instantiations.insert(key, mangled.clone());
                            self.instantiate_struct(name, &substituted_type_args, &mangled)?;
                            // Monomorphized struct - no generic args needed
                            (Vec::<Type>::new(), mangled)
                        }
                    }
                } else {
                    (Vec::new(), name.clone())
                };

                // Build mappings from struct's generic params to actual generic args
                let mut mappings = HashMap::new();
                for (i, p_name) in s_def.generic_params.iter().enumerate() {
                    if i < actual_generic_args.len() {
                        mappings.insert(p_name.clone(), actual_generic_args[i].clone());
                    }
                }

                let mut field_types_map = HashMap::new();
                for f in &s_def.fields {
                    field_types_map.insert(f.name.clone(), self.substitute_type(&f.ty, &mappings));
                }

                let mut typed_fields = Vec::new();
                for (fname, fexpr) in fields {
                    let expected_fty = field_types_map.get(fname).ok_or_else(|| {
                        AnalysisError::new_with_span(
                            &format!("Field '{}' not found in struct '{}'", fname, name),
                            span,
                        )
                        .with_module("infer")
                    })?;
                    self.set_expected_type(Some(expected_fty.clone()));
                    typed_fields.push((fname.clone(), self.infer_expr(fexpr)?));
                }

                Ok(TypedExpr {
                    expr: TypedExprKind::Struct {
                        name: mangled_name.clone(),
                        fields: typed_fields,
                    },
                    ty: Type::Custom {
                        name: mangled_name,
                        generic_args: actual_generic_args,
                        is_exported: s_def.visibility.is_public(),
                    },
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
                let original_expected = self.get_expected_type().cloned();

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

                // If no outer expected type, try to use then branch type for else branch
                if original_expected.is_none() {
                    self.set_expected_type(Some(typed_then.ty.clone()));
                }

                let typed_else = self.infer_expr(else_branch)?;

                // Restore original expected type
                self.set_expected_type(original_expected.clone());

                let mut final_typed_then = typed_then;

                // Handle bidirectional inference: if then was a default type but else is concrete
                if original_expected.is_none()
                    && final_typed_then.ty != typed_else.ty
                    && (final_typed_then.ty == Type::I64 || final_typed_then.ty == Type::F64)
                {
                    // Re-infer then branch with else branch's type
                    self.set_expected_type(Some(typed_else.ty.clone()));
                    if let Some(cap) = capture {
                        self.symbol_table.enter_scope();
                        if let Type::Option(inner_ty) = typed_condition.ty.clone() {
                            self.symbol_table.define(
                                cap.clone(),
                                *inner_ty,
                                Visibility::Private,
                                false,
                            );
                        }
                    }
                    if let Ok(new_then) = self.infer_expr(then_branch) {
                        final_typed_then = new_then;
                    }
                    if capture.is_some() {
                        self.symbol_table.exit_scope();
                    }
                    self.set_expected_type(original_expected);
                }

                // The type of an if expression must be consistent across both branches
                if final_typed_then.ty != typed_else.ty {
                    return Err(AnalysisError::new_with_span(
                        &format!(
                            "If expression branches have incompatible types: '{}' and '{}'",
                            final_typed_then.ty, typed_else.ty
                        ),
                        span,
                    )
                    .with_module("infer"));
                }

                let ty = final_typed_then.ty.clone();

                Ok(TypedExpr {
                    expr: TypedExprKind::If {
                        condition: Box::new(typed_condition),
                        capture: capture.clone(),
                        then_branch: Box::new(final_typed_then),
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
                    let is_imported = self
                        .imports
                        .iter()
                        .any(|(alias, pkg)| pkg == obj_name || alias.as_deref() == Some(obj_name));
                    let is_known_package =
                        obj_name == "std" || obj_name == "io" || obj_name == "os";

                    if is_known_package && !is_imported {
                        return Err(AnalysisError::new_with_span(
                            &format!(
                                "Use of unimported package '{}'. Import it with: import \"{}\"",
                                obj_name, obj_name
                            ),
                            span,
                        )
                        .with_module("infer"));
                    }

                    let is_package =
                        self.symbol_table.resolve(obj_name).is_none() && is_known_package;

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

                if let Type::GenericParam(param_name) = &typed_object.ty {
                    if let Some(interface_names) = self.current_generic_constraints.get(param_name)
                    {
                        for interface_name in interface_names {
                            if let Some(interface_def) = self.interfaces.get(interface_name) {
                                if let Some(method) =
                                    interface_def.methods.iter().find(|m| &m.name == member)
                                {
                                    return Ok(TypedExpr {
                                        expr: TypedExprKind::MemberAccess {
                                            object: Box::new(typed_object),
                                            member: member.clone(),
                                            kind: crate::ast::MemberAccessKind::StructMethod,
                                        },
                                        ty: method.return_ty.clone(),
                                        span: *span,
                                    });
                                }
                            }
                        }
                    }
                }

                let (kind, ty) = if let Some(name) = custom_type_name(&typed_object.ty) {
                    let struct_def_opt = self.structs.get(name).cloned();
                    if let Some(struct_def) = struct_def_opt {
                        // Create mapping from struct generic params to object generic args
                        let mut mappings = HashMap::new();
                        if let Type::Custom { generic_args, .. } = &typed_object.ty {
                            for (i, p_name) in struct_def.generic_params.iter().enumerate() {
                                if let Some(arg) = generic_args.get(i) {
                                    mappings.insert(p_name.clone(), arg.clone());
                                }
                            }
                        }

                        if let Some(method) = struct_def.methods.iter().find(|m| &m.name == member)
                        {
                            let m_mappings = mappings.clone();
                            (
                                crate::ast::MemberAccessKind::StructMethod,
                                self.substitute_type(&method.return_ty, &m_mappings),
                            )
                        } else if let Some(field) =
                            struct_def.fields.iter().find(|f| &f.name == member)
                        {
                            let f_mappings = mappings.clone();
                            (
                                crate::ast::MemberAccessKind::StructField,
                                self.substitute_type(&field.ty, &f_mappings),
                            )
                        } else {
                            return Err(AnalysisError::new_with_span(
                                &format!("Type {} has no member '{}'", name, member),
                                span,
                            )
                            .with_module("infer"));
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
                            return Err(AnalysisError::new_with_span(
                                &format!("Type {} has no member '{}'", name, member),
                                span,
                            )
                            .with_module("infer"));
                        }
                    } else if let Some(error_def) = self.errors.get(name) {
                        if error_def.variants.iter().any(|v| &v.name == member) {
                            (crate::ast::MemberAccessKind::ErrorMember, Type::Error)
                        } else {
                            return Err(AnalysisError::new_with_span(
                                &format!("Type {} has no member '{}'", name, member),
                                span,
                            )
                            .with_module("infer"));
                        }
                    } else {
                        return Err(AnalysisError::new_with_span(
                            &format!("Type {} has no member '{}'", name, member),
                            span,
                        )
                        .with_module("infer"));
                    }
                } else {
                    return Err(AnalysisError::new_with_span(
                        &format!("Type {} has no member '{}'", typed_object.ty, member),
                        span,
                    )
                    .with_module("infer"));
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
            Expr::Try { expr, span } => {
                // Track try expression for function return type validation
                if !self.has_try_expression {
                    self.has_try_expression = true;
                    self.try_expression_span = Some(*span);
                }
                let typed_expr = self.infer_expr(expr)?;

                // Try unwraps the Result type to get the inner type
                let ty = if let Type::Result(inner) = &typed_expr.ty {
                    inner.as_ref().clone()
                } else {
                    return Err(AnalysisError::new_with_span(
                        &format!(
                            "Try expression requires Result type, found {}",
                            typed_expr.ty
                        ),
                        span,
                    )
                    .with_module("infer"));
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
                let result_inner_ty = if let Type::Result(inner) = &typed_expr.ty {
                    inner.as_ref().clone()
                } else {
                    return Err(AnalysisError::new_with_span(
                        &format!(
                            "catch expression requires a Result type, expected Result<T> but found {}",
                            typed_expr.ty
                        ),
                        span,
                    )
                    .with_module("infer"));
                };

                // Handle error variable scope
                let has_error_var = error_var.is_some();
                if let Some(ev) = error_var {
                    self.symbol_table.enter_scope();
                    self.symbol_table
                        .define(ev.clone(), Type::Error, Visibility::Private, false);
                }

                let previous_expected_type = self.get_expected_type().cloned();
                self.set_expected_type(Some(result_inner_ty.clone()));
                let typed_body_result = self.infer_expr(body);
                self.set_expected_type(previous_expected_type);

                // Remove error variable from scope
                if has_error_var {
                    self.symbol_table.exit_scope();
                }

                let typed_body = typed_body_result?;

                if !self.types_compatible(&result_inner_ty, &typed_body.ty) {
                    return Err(AnalysisError::new_with_span(
                        &format!(
                            "catch body type mismatch: expected {}, found {}",
                            result_inner_ty, typed_body.ty
                        ),
                        span,
                    )
                    .with_module("infer"));
                }

                Ok(TypedExpr {
                    expr: TypedExprKind::Catch {
                        expr: Box::new(typed_expr),
                        error_var: error_var.clone(),
                        body: Box::new(typed_body),
                    },
                    ty: result_inner_ty,
                    span: *span,
                })
            }
            Expr::Cast {
                target_type,
                expr,
                span,
            } => {
                // Infer the expression being cast
                let typed_expr = self.infer_expr(expr)?;

                // Check if the cast is valid
                let source_type = &typed_expr.ty;
                let target = target_type;

                // Validate constant cast if the expression is a constant
                self.validate_const_cast(expr, target, span)?;

                // Validate the cast (bit-width check)
                self.validate_cast(source_type, target, span)?;

                // Return the target type
                Ok(TypedExpr {
                    expr: TypedExprKind::Cast {
                        target_type: target_type.clone(),
                        expr: Box::new(typed_expr),
                    },
                    ty: target_type.clone(),
                    span: *span,
                })
            }
            Expr::Dereference { expr, span } => {
                // Infer the pointer expression
                let typed_expr = self.infer_expr(expr)?;

                // Check if it's a pointer type
                let inner_type = match &typed_expr.ty {
                    Type::Pointer(inner) => inner.as_ref().clone(),
                    _ => {
                        return Err(AnalysisError::new_with_span(
                            &format!("Cannot dereference non-pointer type: {}", typed_expr.ty),
                            span,
                        ));
                    }
                };

                // Return the inner type
                Ok(TypedExpr {
                    expr: TypedExprKind::Dereference {
                        expr: Box::new(typed_expr),
                    },
                    ty: inner_type,
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
            UnaryOp::Ref => {
                // Reference operator: &expr creates a pointer to expr
                // For arrays, it creates a slice []T
                if let Type::Array {
                    size: Some(_),
                    element_type,
                } = &expr_ty
                {
                    Ok(Type::Array {
                        size: None,
                        element_type: element_type.clone(),
                    })
                } else {
                    Ok(Type::Pointer(Box::new(expr_ty.clone())))
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
                | Type::F32
                | Type::F64
        )
    }

    /// Validate a type cast
    /// Returns an error if the cast is not allowed (e.g., f64 -> i32 when value might overflow)
    fn validate_cast(&self, source: &Type, target: &Type, span: &Span) -> AnalysisResult<()> {
        // Check if source is a valid type for casting
        let source_is_numeric = self.is_numeric(source);
        let source_is_bool = matches!(source, Type::Bool);
        let target_is_numeric = self.is_numeric(target);
        let target_is_bool = matches!(target, Type::Bool);

        // Allow casts between numeric types, and between bool and numeric
        if source_is_numeric && (target_is_numeric || target_is_bool) {
            // Check for narrowing cast
            if source_is_numeric && target_is_numeric {
                let source_bits = self.get_type_bits(source);
                let target_bits = self.get_type_bits(target);
                if target_bits < source_bits {
                    return Err(AnalysisError::new_with_span(
                        &format!(
                            "Numeric cast from {} to {} is narrowing and potentially unsafe",
                            source, target
                        ),
                        span,
                    )
                    .with_module("infer"));
                }
            }
            return Ok(());
        }

        if source_is_bool && (target_is_numeric || target_is_bool) {
            // Bool to numeric or bool to bool is allowed
            return Ok(());
        }

        // For f64 -> i32 (and similar), we need to check value range at runtime
        // For now, we'll allow it but the codegen should handle it
        if matches!(source, Type::F64) && target_is_numeric {
            // f64 to integer - allowed but needs runtime check
            return Ok(());
        }

        if source_is_numeric && matches!(target, Type::F64) {
            // Integer to f64 - always allowed
            return Ok(());
        }

        // Invalid cast
        Err(AnalysisError::new_with_span(
            &format!("Cannot cast from {} to {}", source, target),
            span,
        ))
    }

    /// Validate a constant cast at compile time
    /// This checks if a constant value can be safely cast to the target type
    fn validate_const_cast(&self, value: &Expr, target: &Type, span: &Span) -> AnalysisResult<()> {
        // Only check constant expressions
        match value {
            Expr::Ident(name, _) => {
                // If it's an identifier, check if it's a known constant
                if let Some(symbol) = self.symbol_table.resolve(name) {
                    if symbol.is_const {
                        if let Some(const_val) = &symbol.const_value {
                            match const_val {
                                ConstantValue::Int(n) => self.check_int_range(*n, target, span)?,
                                ConstantValue::Float(f) => {
                                    self.check_float_range(*f, target, span)?
                                }
                                ConstantValue::Bool(_) => {} // Bool to whatever is usually fine or handled elsewhere
                            }
                        }
                    }
                }
                Ok(())
            }
            Expr::Int(n, _) => self.check_int_range(*n, target, span),
            Expr::Float(f, _) => self.check_float_range(*f, target, span),
            _ => Ok(()),
        }
    }

    fn check_float_range(&self, n: f64, target: &Type, span: &Span) -> AnalysisResult<()> {
        match target {
            Type::I8 => {
                if n < i8::MIN as f64 || n > i8::MAX as f64 || n.is_nan() {
                    return Err(AnalysisError::new_with_span(
                        &format!("Constant {} cannot be safely converted to i8", n),
                        span,
                    ));
                }
            }
            Type::I16 => {
                if n < i16::MIN as f64 || n > i16::MAX as f64 || n.is_nan() {
                    return Err(AnalysisError::new_with_span(
                        &format!("Constant {} cannot be safely converted to i16", n),
                        span,
                    ));
                }
            }
            Type::I32 => {
                if n < i32::MIN as f64 || n > i32::MAX as f64 || n.is_nan() {
                    return Err(AnalysisError::new_with_span(
                        &format!("Constant {} cannot be safely converted to i32", n),
                        span,
                    ));
                }
            }
            Type::I64 => {
                if n < i64::MIN as f64 || n > i64::MAX as f64 || n.is_nan() {
                    return Err(AnalysisError::new_with_span(
                        &format!("Constant {} cannot be safely converted to i64", n),
                        span,
                    ));
                }
            }
            Type::U8 => {
                if n < 0.0 || n > u8::MAX as f64 || n.is_nan() {
                    return Err(AnalysisError::new_with_span(
                        &format!("Constant {} cannot be safely converted to u8", n),
                        span,
                    ));
                }
            }
            Type::U16 => {
                if n < 0.0 || n > u16::MAX as f64 || n.is_nan() {
                    return Err(AnalysisError::new_with_span(
                        &format!("Constant {} cannot be safely converted to u16", n),
                        span,
                    ));
                }
            }
            Type::U32 => {
                if n < 0.0 || n > u32::MAX as f64 || n.is_nan() {
                    return Err(AnalysisError::new_with_span(
                        &format!("Constant {} cannot be safely converted to u32", n),
                        span,
                    ));
                }
            }
            Type::U64 => {
                if n < 0.0 || n > u64::MAX as f64 || n.is_nan() {
                    return Err(AnalysisError::new_with_span(
                        &format!("Constant {} cannot be safely converted to u64", n),
                        span,
                    ));
                }
            }
            Type::F32 => {
                if n < f32::MIN as f64 || n > f32::MAX as f64 {
                    return Err(AnalysisError::new_with_span(
                        &format!("Constant {} exceeds f32 range", n),
                        span,
                    ));
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn check_int_range(&self, n: i64, target: &Type, span: &Span) -> AnalysisResult<()> {
        // Check if the integer fits in the target type
        match target {
            Type::I8 => {
                if n < i8::MIN as i64 || n > i8::MAX as i64 {
                    return Err(AnalysisError::new_with_span(
                        &format!("Constant {} exceeds i8 range ({}, {})", n, i8::MIN, i8::MAX),
                        span,
                    ));
                }
            }
            Type::I16 => {
                if n < i16::MIN as i64 || n > i16::MAX as i64 {
                    return Err(AnalysisError::new_with_span(
                        &format!(
                            "Constant {} exceeds i16 range ({}, {})",
                            n,
                            i16::MIN,
                            i16::MAX
                        ),
                        span,
                    ));
                }
            }
            Type::I32 => {
                if n < i32::MIN as i64 || n > i32::MAX as i64 {
                    return Err(AnalysisError::new_with_span(
                        &format!(
                            "Constant {} exceeds i32 range ({}, {})",
                            n,
                            i32::MIN,
                            i32::MAX
                        ),
                        span,
                    ));
                }
            }
            Type::I64 => {
                // i64 can hold any i64 value
            }
            Type::U8 => {
                if n < 0 || n > u8::MAX as i64 {
                    return Err(AnalysisError::new_with_span(
                        &format!("Constant {} exceeds u8 range (0, {})", n, u8::MAX),
                        span,
                    ));
                }
            }
            Type::U16 => {
                if n < 0 || n > u16::MAX as i64 {
                    return Err(AnalysisError::new_with_span(
                        &format!("Constant {} exceeds u16 range (0, {})", n, u16::MAX),
                        span,
                    ));
                }
            }
            Type::U32 => {
                if n < 0 || n > u32::MAX as i64 {
                    return Err(AnalysisError::new_with_span(
                        &format!("Constant {} exceeds u32 range (0, {})", n, u32::MAX),
                        span,
                    ));
                }
            }
            Type::U64 => {
                if n < 0 {
                    return Err(AnalysisError::new_with_span(
                        &format!("Constant {} is negative but target type is u64", n),
                        span,
                    ));
                }
            }
            Type::F32 | Type::F64 => {
                // Integer to float is always allowed
            }
            Type::Bool => {
                // Can convert any integer to bool
            }
            _ => {}
        }
        Ok(())
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

    fn get_type_bits(&self, ty: &Type) -> u32 {
        match ty {
            Type::I8 | Type::U8 => 8,
            Type::I16 | Type::U16 => 16,
            Type::I32 | Type::U32 | Type::F32 => 32,
            Type::I64 | Type::U64 | Type::F64 => 64,
            _ => 0,
        }
    }

    fn extract_constant_value(&self, typed_expr: &TypedExpr) -> Option<ConstantValue> {
        match &typed_expr.expr {
            TypedExprKind::Int(v) => Some(ConstantValue::Int(*v)),
            TypedExprKind::Float(v) => Some(ConstantValue::Float(*v)),
            TypedExprKind::Bool(v) => Some(ConstantValue::Bool(*v)),
            TypedExprKind::Unary { op, expr } => {
                if let Some(cv) = self.extract_constant_value(expr) {
                    match (op, cv) {
                        (UnaryOp::Neg, ConstantValue::Int(v)) => Some(ConstantValue::Int(-v)),
                        (UnaryOp::Neg, ConstantValue::Float(v)) => Some(ConstantValue::Float(-v)),
                        (UnaryOp::Not, ConstantValue::Int(v)) => {
                            Some(ConstantValue::Int(if v == 0 { 1 } else { 0 }))
                        }
                        (UnaryOp::Not, ConstantValue::Bool(v)) => Some(ConstantValue::Bool(!v)),
                        _ => None,
                    }
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

// ============================================================================
// Public API
// ============================================================================

/// Infer types for a program and produce a type-annotated AST
pub fn infer_types(
    program: &Program,
    symbol_table: SymbolTable,
    structs: HashMap<String, crate::ast::StructDef>,
    interfaces: HashMap<String, crate::ast::InterfaceDef>,
    enums: HashMap<String, crate::ast::EnumDef>,
    errors: HashMap<String, crate::ast::ErrorDef>,
    functions: HashMap<String, crate::ast::FnDef>,
) -> AnalysisResult<TypedProgram> {
    let mut inferrer = TypeInferrer::new(
        symbol_table,
        structs,
        interfaces,
        enums,
        errors,
        functions,
        program.imports.clone(),
    );
    inferrer.infer_program(program)
}

// ============================================================================
// Pretty-Printing for Typed AST
// ============================================================================

pub use crate::ast::{print_indent, AstDump};

fn record_shown_method_names(
    shown_methods: &mut HashSet<String>,
    owner_name: &str,
    method_name: &str,
) {
    shown_methods.insert(method_name.to_string());

    if method_name.starts_with(&format!("{}_", owner_name)) {
        return;
    }

    if let Some((_, original_method_name)) = method_name.split_once('_') {
        shown_methods.insert(format!("{}_{}", owner_name, original_method_name));
    } else {
        shown_methods.insert(format!("{}_{}", owner_name, method_name));
    }
}

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

impl AstDump for TypedInterfaceDef {
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
        // Filter out struct and enum methods from global functions list if they are already shown under their parent
        let mut shown_methods = HashSet::new();
        for s in &self.structs {
            for m in &s.methods {
                record_shown_method_names(&mut shown_methods, &s.name, &m.name);
            }
        }
        for e in &self.enums {
            for m in &e.methods {
                record_shown_method_names(&mut shown_methods, &e.name, &m.name);
            }
        }

        for f in &self.functions {
            if !shown_methods.contains(&f.name) {
                f.dump(indent + 1);
            }
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
        let original_name = self
            .original_name
            .as_ref()
            .map(|name| format!(" ({})", name))
            .unwrap_or_default();
        println!(
            "FnDef: {}{}{} -> {}",
            vis, self.name, original_name, self.return_ty
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
                    format_typed_binding_names(ns, ty)
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
            TypedStmtKind::Assign { target, op, value } => {
                println!("Stmt::Assign");
                print_indent(indent + 1);
                println!("Target: {}", target);
                print_indent(indent + 1);
                println!("Op: {:?}", op);
                print_indent(indent + 1);
                println!("Value (ty: {}):", value.ty);
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

                // Determine element type from iterable
                let element_type = match &iterable.ty {
                    Type::Array { element_type, .. } => element_type.as_ref().clone(),
                    _ => Type::I64,
                };

                if let Some(v) = var_name {
                    print_indent(indent + 1);
                    println!("Var: {} (ty: {})", v, element_type);
                }
                if let Some(i) = index_var {
                    print_indent(indent + 1);
                    println!("Index: {} (ty: i64)", i);
                }
                if let Some(c) = capture {
                    print_indent(indent + 1);
                    println!("Capture: {} (ty: {})", c, element_type);
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
            TypedStmtKind::Continue { label } => {
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
            TypedExprKind::Index { object, index } => {
                println!("Expr::Index: [] (ty: {})", self.ty);
                object.dump(indent + 1);
                index.dump(indent + 1);
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
                target_ty: _,
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
            TypedExprKind::Cast { target_type, expr } => {
                println!("Expr::Cast to {} (ty: {})", target_type, self.ty);
                expr.dump(indent + 1);
            }
            TypedExprKind::Dereference { expr } => {
                println!("Expr::Dereference (ty: {})", self.ty);
                expr.dump(indent + 1);
            }
            TypedExprKind::Intrinsic { name, args } => {
                println!("Expr::Intrinsic: {} (ty: {})", name, self.ty);
                for arg in args {
                    arg.dump(indent + 1);
                }
            }
            TypedExprKind::TypeLiteral(ty) => {
                println!("Expr::TypeLiteral: {} (ty: {})", ty, self.ty);
            }
        }
    }
}
