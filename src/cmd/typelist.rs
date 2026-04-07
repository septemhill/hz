//! # Typelist Command
//!
//! This module implements the `typelist` CLI command which lists all types
//! in a target project along with their unique type IDs.

use crate::ast::Type;
use crate::parser::parse;
use crate::sema::{typelist::TypeRegistry, SemanticAnalyzer};

/// Run the typelist command - lists all types in the source file with their IDs
pub fn run_typelist_command(
    source: &str,
    _std_path: Option<std::path::PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Parse source code
    let mut program = parse(source)?;

    // Initialize std library
    let mut stdlib = crate::stdlib::StdLib::new();
    let stdlib_path = crate::cmd::resolve_std_path(_std_path);
    stdlib.set_std_path(stdlib_path.to_str().unwrap());
    let _ = stdlib.preload_builtins();

    // Load imported packages
    for (_, package_name) in &program.imports {
        let _ = stdlib.load_package(package_name);
    }

    // Run semantic analysis to get typed program with monomorphized generics
    let mut analyzer = SemanticAnalyzer::new();
    match analyzer.analyze_with_stdlib(&mut program, Some(&stdlib), true) {
        Ok(_) => {
            // Get the typed program (contains monomorphized types)
            if let Some(typed_program) = analyzer.get_typed_program() {
                list_types_from_typed_program(typed_program);
            } else {
                // Fallback to AST-level types
                list_types_from_ast_program(&program);
            }
        }
        Err(e) => {
            // Even with errors, try to get the typed program
            if let Some(typed_program) = analyzer.get_typed_program() {
                eprintln!(
                    "Warning: Semantic analysis had errors, but showing types anyway: {}",
                    e
                );
                list_types_from_typed_program(typed_program);
            } else {
                eprintln!("Error: Semantic analysis failed: {}", e);
                list_types_from_ast_program(&program);
            }
        }
    }

    Ok(())
}

/// List types from a typed program (includes monomorphized generic types)
fn list_types_from_typed_program(typed_program: &crate::sema::infer::TypedProgram) {
    let mut registry = TypeRegistry::new();
    let mut type_list: Vec<(u32, Type)> = Vec::new();

    // Collect types from typed functions (monomorphized)
    for fn_def in &typed_program.functions {
        // Return type - only add if it doesn't have non-generic generic_args
        if is_concrete_type(&fn_def.return_ty) {
            let id = registry.get_type_id(&fn_def.return_ty);
            type_list.push((id, fn_def.return_ty.clone()));
        }

        // Parameter types
        for param in &fn_def.params {
            if is_concrete_type(&param.ty) {
                let id = registry.get_type_id(&param.ty);
                type_list.push((id, param.ty.clone()));
            }
        }

        // Collect types from body statements
        for stmt in &fn_def.body {
            collect_typed_stmt_types(stmt, &mut registry, &mut type_list);
        }
    }

    // Collect types from typed structs (monomorphized)
    for struct_def in &typed_program.structs {
        // Struct type
        let custom_ty = Type::Custom {
            name: struct_def.name.clone(),
            generic_args: vec![],
            is_exported: struct_def.visibility == crate::ast::Visibility::Public,
        };
        let id = registry.get_type_id(&custom_ty);
        type_list.push((id, custom_ty));

        // Field types
        for field in &struct_def.fields {
            if is_concrete_type(&field.ty) {
                let id = registry.get_type_id(&field.ty);
                type_list.push((id, field.ty.clone()));
            }
        }
    }

    // Collect types from typed enums
    for enum_def in &typed_program.enums {
        let custom_ty = Type::Custom {
            name: enum_def.name.clone(),
            generic_args: vec![],
            is_exported: enum_def.visibility == crate::ast::Visibility::Public,
        };
        let id = registry.get_type_id(&custom_ty);
        type_list.push((id, custom_ty));

        // Variant associated types
        for variant in &enum_def.variants {
            for ty in &variant.associated_types {
                if is_concrete_type(ty) {
                    let id = registry.get_type_id(ty);
                    type_list.push((id, ty.clone()));
                }
            }
        }
    }

    // Collect types from typed errors
    for error_def in &typed_program.errors {
        let custom_ty = Type::Custom {
            name: error_def.name.clone(),
            generic_args: vec![],
            is_exported: error_def.visibility == crate::ast::Visibility::Public,
        };
        let id = registry.get_type_id(&custom_ty);
        type_list.push((id, custom_ty));

        // Variant associated types
        for variant in &error_def.variants {
            for ty in &variant.associated_types {
                if is_concrete_type(ty) {
                    let id = registry.get_type_id(ty);
                    type_list.push((id, ty.clone()));
                }
            }
        }
    }

    // Sort and dedupe by type ID
    type_list.sort_by_key(|(id, _)| *id);
    type_list.dedup_by_key(|(id, _)| *id);

    // Print the type list
    print_type_list(type_list);
}

/// Check if a type is a concrete type (not a generic type instantiation)
fn is_concrete_type(ty: &Type) -> bool {
    match ty {
        Type::Custom { generic_args, .. } => {
            // Only include if generic_args are empty or only GenericParam
            generic_args
                .iter()
                .all(|arg| matches!(arg, Type::GenericParam(_)))
        }
        Type::Function {
            params,
            return_type,
        } => params.iter().all(|p| is_concrete_type(p)) && is_concrete_type(return_type),
        Type::Pointer(inner) | Type::Option(inner) | Type::Result(inner) => is_concrete_type(inner),
        Type::Array { element_type, .. } => is_concrete_type(element_type),
        Type::Tuple(types) => types.iter().all(is_concrete_type),
        _ => true,
    }
}

/// Collect types from typed statements
fn collect_typed_stmt_types(
    stmt: &crate::sema::infer::TypedStmt,
    registry: &mut TypeRegistry,
    type_list: &mut Vec<(u32, Type)>,
) {
    use crate::sema::infer::TypedStmtKind;

    match &stmt.stmt {
        TypedStmtKind::Expr { expr } => {
            collect_typed_expr_types(expr, registry, type_list);
        }
        TypedStmtKind::Let { ty, value, .. } => {
            // Only add the let type if it's concrete
            if is_concrete_type(ty) {
                let id = registry.get_type_id(ty);
                type_list.push((id, ty.clone()));
            }
            if let Some(v) = value {
                collect_typed_expr_types(v, registry, type_list);
            }
        }
        TypedStmtKind::Assign { value, .. } => {
            collect_typed_expr_types(value, registry, type_list);
        }
        TypedStmtKind::Return { value, .. } => {
            if let Some(v) = value {
                collect_typed_expr_types(v, registry, type_list);
            }
        }
        TypedStmtKind::Block { stmts, .. } => {
            for s in stmts {
                collect_typed_stmt_types(s, registry, type_list);
            }
        }
        TypedStmtKind::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            collect_typed_expr_types(condition, registry, type_list);
            collect_typed_stmt_types(then_branch, registry, type_list);
            if let Some(eb) = else_branch {
                collect_typed_stmt_types(eb, registry, type_list);
            }
        }
        TypedStmtKind::While {
            condition, body, ..
        } => {
            collect_typed_expr_types(condition, registry, type_list);
            collect_typed_stmt_types(body, registry, type_list);
        }
        TypedStmtKind::For { iterable, body, .. } => {
            collect_typed_expr_types(iterable, registry, type_list);
            collect_typed_stmt_types(body, registry, type_list);
        }
        TypedStmtKind::Switch {
            condition, cases, ..
        } => {
            collect_typed_expr_types(condition, registry, type_list);
            for case in cases {
                collect_typed_stmt_types(&case.body, registry, type_list);
            }
        }
        TypedStmtKind::Defer { stmt, .. } | TypedStmtKind::DeferBang { stmt, .. } => {
            collect_typed_stmt_types(stmt, registry, type_list);
        }
        TypedStmtKind::Import { .. }
        | TypedStmtKind::Break { .. }
        | TypedStmtKind::Continue { .. } => {}
    }
}

/// Collect types from typed expressions
fn collect_typed_expr_types(
    expr: &crate::sema::infer::TypedExpr,
    registry: &mut TypeRegistry,
    type_list: &mut Vec<(u32, Type)>,
) {
    use crate::sema::infer::TypedExprKind;

    // Add the type of this expression only if it's concrete
    if is_concrete_type(&expr.ty) {
        let id = registry.get_type_id(&expr.ty);
        type_list.push((id, expr.ty.clone()));
    }

    match &expr.expr {
        TypedExprKind::Int(_) => {}
        TypedExprKind::Float(_) => {}
        TypedExprKind::Bool(_) => {}
        TypedExprKind::String(_) => {}
        TypedExprKind::Char(_) => {}
        TypedExprKind::Null => {}
        TypedExprKind::Ident(_) => {}

        TypedExprKind::Binary { left, right, .. } => {
            collect_typed_expr_types(left, registry, type_list);
            collect_typed_expr_types(right, registry, type_list);
        }
        TypedExprKind::Unary { expr: inner, .. } => {
            collect_typed_expr_types(inner, registry, type_list);
        }
        TypedExprKind::Call { args, .. } => {
            for arg in args {
                collect_typed_expr_types(arg, registry, type_list);
            }
        }
        TypedExprKind::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            collect_typed_expr_types(condition, registry, type_list);
            collect_typed_expr_types(then_branch, registry, type_list);
            collect_typed_expr_types(else_branch, registry, type_list);
        }
        TypedExprKind::Block { stmts, .. } => {
            for stmt in stmts {
                collect_typed_stmt_types(stmt, registry, type_list);
            }
        }
        TypedExprKind::MemberAccess { object, .. } => {
            collect_typed_expr_types(object, registry, type_list);
        }
        TypedExprKind::TupleIndex { tuple, .. } => {
            collect_typed_expr_types(tuple, registry, type_list);
        }
        TypedExprKind::Struct { fields, .. } => {
            for (_, field_expr) in fields {
                collect_typed_expr_types(field_expr, registry, type_list);
            }
        }
        TypedExprKind::Array(elements) => {
            for elem in elements {
                collect_typed_expr_types(elem, registry, type_list);
            }
        }
        TypedExprKind::Tuple(elements) => {
            for elem in elements {
                collect_typed_expr_types(elem, registry, type_list);
            }
        }
        TypedExprKind::Try { expr, .. } => {
            collect_typed_expr_types(expr, registry, type_list);
        }
        TypedExprKind::Catch { expr, body, .. } => {
            collect_typed_expr_types(expr, registry, type_list);
            collect_typed_expr_types(body, registry, type_list);
        }
        TypedExprKind::Cast { expr, .. } => {
            collect_typed_expr_types(expr, registry, type_list);
        }
        TypedExprKind::Dereference { expr, .. } => {
            collect_typed_expr_types(expr, registry, type_list);
        }
        TypedExprKind::Index { object, index, .. } => {
            collect_typed_expr_types(object, registry, type_list);
            collect_typed_expr_types(index, registry, type_list);
        }
        TypedExprKind::Intrinsic { args, .. } => {
            for arg in args {
                collect_typed_expr_types(arg, registry, type_list);
            }
        }
        TypedExprKind::TypeLiteral(ty) => {
            collect_type(ty, registry, type_list);
        }
    }
}

/// List types from AST-level program (for fallback or non-generic programs)
fn list_types_from_ast_program(program: &crate::ast::Program) {
    let mut registry = TypeRegistry::new();
    let mut type_list: Vec<(u32, Type)> = Vec::new();

    // Collect types from structs
    for struct_def in &program.structs {
        let custom_ty = Type::Custom {
            name: struct_def.name.clone(),
            generic_args: struct_def
                .generic_params
                .iter()
                .map(|p| Type::GenericParam(p.clone()))
                .collect(),
            is_exported: struct_def.visibility == crate::ast::Visibility::Public,
        };
        let id = registry.get_type_id(&custom_ty);
        type_list.push((id, custom_ty));

        // Field types
        for field in &struct_def.fields {
            collect_type(&field.ty, &mut registry, &mut type_list);
        }

        // Method types
        for method in &struct_def.methods {
            collect_fn_types(method, &mut registry, &mut type_list);
        }
    }

    // Collect types from enums
    for enum_def in &program.enums {
        let custom_ty = Type::Custom {
            name: enum_def.name.clone(),
            generic_args: enum_def
                .generic_params
                .iter()
                .map(|p| Type::GenericParam(p.clone()))
                .collect(),
            is_exported: enum_def.visibility == crate::ast::Visibility::Public,
        };
        let id = registry.get_type_id(&custom_ty);
        type_list.push((id, custom_ty));

        // Variant types
        for variant in &enum_def.variants {
            for ty in &variant.associated_types {
                collect_type(ty, &mut registry, &mut type_list);
            }
        }

        // Method types
        for method in &enum_def.methods {
            collect_fn_types(method, &mut registry, &mut type_list);
        }
    }

    // Collect types from functions
    for fn_def in &program.functions {
        collect_fn_types(fn_def, &mut registry, &mut type_list);
    }

    // Collect types from errors
    for error_def in &program.errors {
        let custom_ty = Type::Custom {
            name: error_def.name.clone(),
            generic_args: vec![],
            is_exported: error_def.visibility == crate::ast::Visibility::Public,
        };
        let id = registry.get_type_id(&custom_ty);
        type_list.push((id, custom_ty));

        for variant in &error_def.variants {
            for ty in &variant.associated_types {
                collect_type(ty, &mut registry, &mut type_list);
            }
        }
    }

    // Sort and dedupe by type ID
    type_list.sort_by_key(|(id, _)| *id);
    type_list.dedup_by_key(|(id, _)| *id);

    // Print the type list
    print_type_list(type_list);
}

/// Collect types from a function definition
fn collect_fn_types(
    fn_def: &crate::ast::FnDef,
    registry: &mut TypeRegistry,
    type_list: &mut Vec<(u32, Type)>,
) {
    // Return type
    collect_type(&fn_def.return_ty, registry, type_list);

    // Parameter types
    for param in &fn_def.params {
        collect_type(&param.ty, registry, type_list);
    }

    // Body types
    for stmt in &fn_def.body {
        collect_stmt_types(stmt, registry, type_list);
    }
}

/// Collect types from a statement
fn collect_stmt_types(
    stmt: &crate::ast::Stmt,
    registry: &mut TypeRegistry,
    type_list: &mut Vec<(u32, Type)>,
) {
    match stmt {
        crate::ast::Stmt::Let { ty, value, .. } => {
            if let Some(t) = ty {
                collect_type(t, registry, type_list);
            }
            if let Some(v) = value {
                collect_expr_types(v, registry, type_list);
            }
        }
        crate::ast::Stmt::Assign { value, .. } => {
            collect_expr_types(value, registry, type_list);
        }
        crate::ast::Stmt::Return { value, .. } => {
            if let Some(v) = value {
                collect_expr_types(v, registry, type_list);
            }
        }
        crate::ast::Stmt::Expr { expr, .. } => {
            collect_expr_types(expr, registry, type_list);
        }
        crate::ast::Stmt::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            collect_expr_types(condition, registry, type_list);
            collect_stmt_types(then_branch, registry, type_list);
            if let Some(eb) = else_branch {
                collect_stmt_types(eb, registry, type_list);
            }
        }
        crate::ast::Stmt::For { iterable, body, .. } => {
            collect_expr_types(iterable, registry, type_list);
            collect_stmt_types(body, registry, type_list);
        }
        crate::ast::Stmt::Switch {
            condition, cases, ..
        } => {
            collect_expr_types(condition, registry, type_list);
            for case in cases {
                collect_stmt_types(&case.body, registry, type_list);
            }
        }
        crate::ast::Stmt::Block { stmts, .. } => {
            for s in stmts {
                collect_stmt_types(s, registry, type_list);
            }
        }
        crate::ast::Stmt::Defer { stmt, .. } | crate::ast::Stmt::DeferBang { stmt, .. } => {
            collect_stmt_types(stmt, registry, type_list);
        }
        crate::ast::Stmt::Import { .. }
        | crate::ast::Stmt::Break { .. }
        | crate::ast::Stmt::Continue { .. } => {}
    }
}

/// Collect types from an expression
fn collect_expr_types(
    expr: &crate::ast::Expr,
    registry: &mut TypeRegistry,
    type_list: &mut Vec<(u32, Type)>,
) {
    match expr {
        crate::ast::Expr::Int(_, _)
        | crate::ast::Expr::Float(_, _)
        | crate::ast::Expr::Bool(_, _)
        | crate::ast::Expr::String(_, _)
        | crate::ast::Expr::Char(_, _)
        | crate::ast::Expr::Null(_)
        | crate::ast::Expr::Ident(_, _) => {}

        crate::ast::Expr::Binary { left, right, .. } => {
            collect_expr_types(left, registry, type_list);
            collect_expr_types(right, registry, type_list);
        }
        crate::ast::Expr::Unary { expr: inner, .. } => {
            collect_expr_types(inner, registry, type_list);
        }
        crate::ast::Expr::Call { args, .. } => {
            for arg in args {
                collect_expr_types(arg, registry, type_list);
            }
        }
        crate::ast::Expr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            collect_expr_types(condition, registry, type_list);
            collect_expr_types(then_branch, registry, type_list);
            collect_expr_types(else_branch, registry, type_list);
        }
        crate::ast::Expr::Block { stmts, .. } => {
            for stmt in stmts {
                collect_stmt_types(stmt, registry, type_list);
            }
        }
        crate::ast::Expr::MemberAccess { object, .. } => {
            collect_expr_types(object, registry, type_list);
        }
        crate::ast::Expr::TupleIndex { tuple, .. } => {
            collect_expr_types(tuple, registry, type_list);
        }
        crate::ast::Expr::Struct { fields, .. } => {
            for (_, field_expr) in fields {
                collect_expr_types(field_expr, registry, type_list);
            }
        }
        crate::ast::Expr::Array(elements, _, _) => {
            for elem in elements {
                collect_expr_types(elem, registry, type_list);
            }
        }
        crate::ast::Expr::Tuple(elements, _) => {
            for elem in elements {
                collect_expr_types(elem, registry, type_list);
            }
        }
        crate::ast::Expr::Try { expr, .. } => {
            collect_expr_types(expr, registry, type_list);
        }
        crate::ast::Expr::Catch { expr, body, .. } => {
            collect_expr_types(expr, registry, type_list);
            collect_expr_types(body, registry, type_list);
        }
        crate::ast::Expr::Cast { expr, .. } => {
            collect_expr_types(expr, registry, type_list);
        }
        crate::ast::Expr::Dereference { expr, .. } => {
            collect_expr_types(expr, registry, type_list);
        }
        crate::ast::Expr::Index { object, index, .. } => {
            collect_expr_types(object, registry, type_list);
            collect_expr_types(index, registry, type_list);
        }
        crate::ast::Expr::Intrinsic { args, .. } => {
            for arg in args {
                collect_expr_types(arg, registry, type_list);
            }
        }
        crate::ast::Expr::TypeLiteral(ty, _) => {
            collect_type(ty, registry, type_list);
        }
    }
}

/// Collect types from a type annotation
fn collect_type(ty: &Type, registry: &mut TypeRegistry, type_list: &mut Vec<(u32, Type)>) {
    let id = registry.get_type_id(ty);
    type_list.push((id, ty.clone()));

    match ty {
        Type::Pointer(inner) => collect_type(inner, registry, type_list),
        Type::Option(inner) => collect_type(inner, registry, type_list),
        Type::Result(inner) => collect_type(inner, registry, type_list),
        Type::Array { element_type, .. } => collect_type(element_type, registry, type_list),
        Type::Tuple(ts) => {
            for t in ts {
                collect_type(t, registry, type_list);
            }
        }
        Type::Custom { generic_args, .. } => {
            for arg in generic_args {
                collect_type(arg, registry, type_list);
            }
        }
        Type::Function {
            params,
            return_type,
        } => {
            for p in params {
                collect_type(p, registry, type_list);
            }
            collect_type(return_type, registry, type_list);
        }
        _ => {}
    }
}

/// Print the type list
fn print_type_list(mut type_list: Vec<(u32, Type)>) {
    // Sort by type ID
    type_list.sort_by_key(|(id, _)| *id);

    // Print
    println!("Type ID | Type");
    println!("--------|------");
    let type_count = type_list.len();
    for (id, ty) in type_list {
        println!("{:7} | {}", id, format_type(&ty));
    }

    println!();
    println!("Total types: {}", type_count);
}

/// Format a type for display
fn format_type(ty: &Type) -> String {
    match ty {
        Type::I8 => "i8".to_string(),
        Type::I16 => "i16".to_string(),
        Type::I32 => "i32".to_string(),
        Type::I64 => "i64".to_string(),
        Type::U8 => "u8".to_string(),
        Type::U16 => "u16".to_string(),
        Type::U32 => "u32".to_string(),
        Type::U64 => "u64".to_string(),
        Type::F32 => "f32".to_string(),
        Type::F64 => "f64".to_string(),
        Type::ImmInt => "ImmInt".to_string(),
        Type::ImmFloat => "ImmFloat".to_string(),
        Type::Bool => "bool".to_string(),
        Type::Void => "void".to_string(),
        Type::RawPtr => "rawptr".to_string(),
        Type::SelfType => "Self".to_string(),
        Type::Pointer(inner) => format!("*{}", format_type(inner)),
        Type::Option(inner) => format!("?{}", format_type(inner)),
        Type::Tuple(types) => {
            let inner = types
                .iter()
                .map(|t| format_type(t))
                .collect::<Vec<_>>()
                .join(", ");
            format!("({})", inner)
        }
        Type::Custom {
            name, generic_args, ..
        } => {
            if generic_args.is_empty() {
                name.clone()
            } else {
                let args = generic_args
                    .iter()
                    .map(|t| format_type(t))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{}<{}>", name, args)
            }
        }
        Type::GenericParam(name) => name.clone(),
        Type::Array { size, element_type } => {
            if let Some(size) = size {
                format!("[{}]{}", size, format_type(element_type))
            } else {
                format!("[]{}", format_type(element_type))
            }
        }
        Type::Error => "error".to_string(),
        Type::Result(inner) => format!("{}!", format_type(inner)),
        Type::Const(inner) => format!("const {}", format_type(inner)),
        Type::Function {
            params,
            return_type,
        } => {
            let params_str = params
                .iter()
                .map(|t| format_type(t))
                .collect::<Vec<_>>()
                .join(", ");
            format!("fn({}) {}", params_str, format_type(return_type))
        }
        Type::VarArgs => "varargs".to_string(),
        Type::VarArgsPack(types) => {
            let inner = types
                .iter()
                .map(|t| format_type(t))
                .collect::<Vec<_>>()
                .join(", ");
            format!("varargs({})", inner)
        }
        Type::Package(name) => format!("package({})", name),
    }
}
