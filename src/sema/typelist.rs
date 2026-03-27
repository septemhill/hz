// Type Registry Module
// Provides unique type IDs for each unique type in the program

use crate::ast::Type;
use std::collections::HashMap;

/// A registry that assigns unique IDs to types
pub struct TypeRegistry {
    /// Maps type to its unique ID
    type_to_id: HashMap<Type, u32>,
    /// Counter for generating unique IDs
    next_id: u32,
}

impl TypeRegistry {
    pub fn new() -> Self {
        TypeRegistry {
            type_to_id: HashMap::new(),
            next_id: 0,
        }
    }

    /// Get or create a type ID for the given type
    pub fn get_type_id(&mut self, ty: &Type) -> u32 {
        // First, normalize the type (simplify for comparison)
        let normalized = self.normalize_type(ty);

        // Check if we already have an ID for this type
        if let Some(&id) = self.type_to_id.get(&normalized) {
            return id;
        }

        // Assign a new ID
        let id = self.next_id;
        self.next_id += 1;
        self.type_to_id.insert(normalized, id);
        id
    }

    /// Normalize a type for comparison (simplify nested types)
    fn normalize_type(&self, ty: &Type) -> Type {
        match ty {
            Type::Pointer(inner) => Type::Pointer(Box::new(self.normalize_type(inner))),
            Type::Option(inner) => Type::Option(Box::new(self.normalize_type(inner))),
            Type::Result(inner) => Type::Result(Box::new(self.normalize_type(inner))),
            Type::Array { size, element_type } => Type::Array {
                size: *size,
                element_type: Box::new(self.normalize_type(element_type)),
            },
            Type::Tuple(types) => {
                Type::Tuple(types.iter().map(|t| self.normalize_type(t)).collect())
            }
            Type::Custom {
                name,
                generic_args,
                is_exported,
            } => Type::Custom {
                name: name.clone(),
                generic_args: generic_args
                    .iter()
                    .map(|t| self.normalize_type(t))
                    .collect(),
                is_exported: *is_exported,
            },
            Type::Function {
                params,
                return_type,
            } => Type::Function {
                params: params.iter().map(|t| self.normalize_type(t)).collect(),
                return_type: Box::new(self.normalize_type(return_type)),
            },
            other => other.clone(),
        }
    }

    /// Get all registered types with their IDs
    pub fn get_all_types(&self) -> Vec<(u32, Type)> {
        let mut types: Vec<(u32, Type)> = self
            .type_to_id
            .iter()
            .map(|(ty, &id)| (id, ty.clone()))
            .collect();
        types.sort_by_key(|(id, _)| *id);
        types
    }

    /// Get the total number of registered types
    pub fn len(&self) -> usize {
        self.type_to_id.len()
    }
}

impl Default for TypeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Collect all types from a program
pub fn collect_types_from_program(program: &crate::ast::Program) -> Vec<Type> {
    let mut types = Vec::new();

    // Collect types from structs
    for struct_def in &program.structs {
        // Add the struct's own type
        let generic_args: Vec<Type> = struct_def
            .generic_params
            .iter()
            .map(|p| Type::GenericParam(p.clone()))
            .collect();
        types.push(Type::Custom {
            name: struct_def.name.clone(),
            generic_args,
            is_exported: struct_def.visibility == crate::ast::Visibility::Public,
        });

        // Add field types
        for field in &struct_def.fields {
            collect_types_from_type(&field.ty, &mut types);
        }

        // Add method types
        for method in &struct_def.methods {
            collect_fn_types(method, &mut types);
        }
    }

    // Collect types from enums
    for enum_def in &program.enums {
        let generic_args: Vec<Type> = enum_def
            .generic_params
            .iter()
            .map(|p| Type::GenericParam(p.clone()))
            .collect();
        types.push(Type::Custom {
            name: enum_def.name.clone(),
            generic_args,
            is_exported: enum_def.visibility == crate::ast::Visibility::Public,
        });

        // Add variant associated types
        for variant in &enum_def.variants {
            for field in &variant.associated_types {
                collect_types_from_type(field, &mut types);
            }
        }

        // Add method types
        for method in &enum_def.methods {
            collect_fn_types(method, &mut types);
        }
    }

    // Collect types from functions
    for fn_def in &program.functions {
        collect_fn_types(fn_def, &mut types);
    }

    // Collect types from errors
    for error_def in &program.errors {
        types.push(Type::Custom {
            name: error_def.name.clone(),
            generic_args: vec![],
            is_exported: error_def.visibility == crate::ast::Visibility::Public,
        });

        // Add error variant associated types
        for variant in &error_def.variants {
            for field in &variant.associated_types {
                collect_types_from_type(field, &mut types);
            }
        }

        // Add union types if present
        if let Some(union_types) = &error_def.union_types {
            for ty in union_types {
                collect_types_from_type(ty, &mut types);
            }
        }
    }

    types
}

/// Collect types from a function definition
fn collect_fn_types(fn_def: &crate::ast::FnDef, types: &mut Vec<Type>) {
    // Add return type
    collect_types_from_type(&fn_def.return_ty, types);

    // Add parameter types
    for param in &fn_def.params {
        collect_types_from_type(&param.ty, types);
    }

    // Add body types (for statements and expressions)
    for stmt in &fn_def.body {
        collect_types_from_stmt(stmt, types);
    }
}

/// Collect types from a statement
fn collect_types_from_stmt(stmt: &crate::ast::Stmt, types: &mut Vec<Type>) {
    match stmt {
        crate::ast::Stmt::Let { ty, value, .. } => {
            if let Some(t) = ty {
                collect_types_from_type(t, types);
            }
            if let Some(v) = value {
                collect_types_from_expr(v, types);
            }
        }
        crate::ast::Stmt::Assign { value, .. } => {
            collect_types_from_expr(value, types);
        }
        crate::ast::Stmt::Return { value, .. } => {
            if let Some(v) = value {
                collect_types_from_expr(v, types);
            }
        }
        crate::ast::Stmt::Expr { expr, .. } => {
            collect_types_from_expr(expr, types);
        }
        crate::ast::Stmt::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            collect_types_from_expr(condition, types);
            collect_types_from_stmt(then_branch, types);
            if let Some(eb) = else_branch {
                collect_types_from_stmt(eb, types);
            }
        }
        crate::ast::Stmt::For { iterable, body, .. } => {
            collect_types_from_expr(iterable, types);
            collect_types_from_stmt(body, types);
        }
        crate::ast::Stmt::Switch {
            condition, cases, ..
        } => {
            collect_types_from_expr(condition, types);
            for case in cases {
                collect_types_from_stmt(&case.body, types);
            }
        }
        crate::ast::Stmt::Block { stmts, .. } => {
            for s in stmts {
                collect_types_from_stmt(s, types);
            }
        }
        crate::ast::Stmt::Defer { stmt, .. } | crate::ast::Stmt::DeferBang { stmt, .. } => {
            collect_types_from_stmt(stmt, types);
        }
        crate::ast::Stmt::Import { .. } | crate::ast::Stmt::Break { .. } => {}
    }
}

/// Collect types from an expression
fn collect_types_from_expr(expr: &crate::ast::Expr, types: &mut Vec<Type>) {
    match expr {
        crate::ast::Expr::Int(_, _)
        | crate::ast::Expr::Float(_, _)
        | crate::ast::Expr::Bool(_, _)
        | crate::ast::Expr::String(_, _)
        | crate::ast::Expr::Char(_, _)
        | crate::ast::Expr::Null(_)
        | crate::ast::Expr::Ident(_, _) => {}

        crate::ast::Expr::Binary { left, right, .. } => {
            collect_types_from_expr(left, types);
            collect_types_from_expr(right, types);
        }
        crate::ast::Expr::Unary { expr: inner, .. } => {
            collect_types_from_expr(inner, types);
        }
        crate::ast::Expr::Call { args, .. } => {
            for arg in args {
                collect_types_from_expr(arg, types);
            }
        }
        crate::ast::Expr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            collect_types_from_expr(condition, types);
            collect_types_from_expr(then_branch, types);
            collect_types_from_expr(else_branch, types);
        }
        crate::ast::Expr::Block { stmts, .. } => {
            for stmt in stmts {
                collect_types_from_stmt(stmt, types);
            }
        }
        crate::ast::Expr::MemberAccess { object, .. } => {
            collect_types_from_expr(object, types);
        }
        crate::ast::Expr::TupleIndex { tuple, .. } => {
            collect_types_from_expr(tuple, types);
        }
        crate::ast::Expr::Struct { fields, .. } => {
            for (_, field_expr) in fields {
                collect_types_from_expr(field_expr, types);
            }
        }
        crate::ast::Expr::Array(elements, _, _) => {
            for elem in elements {
                collect_types_from_expr(elem, types);
            }
        }
        crate::ast::Expr::Tuple(elements, _) => {
            for elem in elements {
                collect_types_from_expr(elem, types);
            }
        }
        crate::ast::Expr::Try { expr, .. } => {
            collect_types_from_expr(expr, types);
        }
        crate::ast::Expr::Catch { expr, body, .. } => {
            collect_types_from_expr(expr, types);
            collect_types_from_expr(body, types);
        }
        crate::ast::Expr::Cast { expr, .. } => {
            collect_types_from_expr(expr, types);
        }
    }
}

/// Collect types from a type annotation
fn collect_types_from_type(ty: &Type, types: &mut Vec<Type>) {
    // Avoid duplicates
    if !types.contains(ty) {
        types.push(ty.clone());
    }

    match ty {
        Type::Pointer(inner) => collect_types_from_type(inner, types),
        Type::Option(inner) => collect_types_from_type(inner, types),
        Type::Result(inner) => collect_types_from_type(inner, types),
        Type::Array { element_type, .. } => collect_types_from_type(element_type, types),
        Type::Tuple(ts) => {
            for t in ts {
                collect_types_from_type(t, types);
            }
        }
        Type::Custom { generic_args, .. } => {
            for arg in generic_args {
                collect_types_from_type(arg, types);
            }
        }
        Type::Function {
            params,
            return_type,
        } => {
            for p in params {
                collect_types_from_type(p, types);
            }
            collect_types_from_type(return_type, types);
        }
        _ => {}
    }
}
