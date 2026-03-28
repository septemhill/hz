//! # TreeShaker - Dead Code Elimination at Compile Time
//!
//! This module provides tree-shaking functionality to remove unused code at compile time.
//! It handles:
//! - Basic dead code elimination (unused functions, structs, enums)
//! - Generic type instantiation tracking (which generic types are actually used)
//! - Interface implementation tracking (which impl blocks are actually used)

use crate::ast::{Expr, FnDef, Program, Stmt, Type};
use crate::sema::symbol::SymbolTable;
use std::collections::{HashMap, HashSet};

/// TreeShaker performs dead code elimination on the AST level
pub struct TreeShaker {
    /// Symbols that are reachable (used)
    reachable_functions: HashSet<String>,
    reachable_structs: HashSet<String>,
    reachable_enums: HashSet<String>,
    reachable_errors: HashSet<String>,
    /// Generic instantiations that are used (e.g., "Compose<u8>")
    reachable_generic_instantiations: HashSet<String>,
    /// Track which generic functions are used with specific type arguments
    reachable_generic_functions: HashSet<String>,
    /// Symbol table for looking up symbols
    symbol_table: SymbolTable,
}

impl TreeShaker {
    /// Create a new TreeShaker
    pub fn new(symbol_table: SymbolTable) -> Self {
        TreeShaker {
            reachable_functions: HashSet::new(),
            reachable_structs: HashSet::new(),
            reachable_enums: HashSet::new(),
            reachable_errors: HashSet::new(),
            reachable_generic_instantiations: HashSet::new(),
            reachable_generic_functions: HashSet::new(),
            symbol_table,
        }
    }

    /// Run tree-shaking on the program
    pub fn shake(&mut self, program: &mut Program) {
        // Step 1: Mark all entry points as reachable
        // Entry points include: main function and any public/exported functions
        self.mark_entry_points(program);

        // Step 2: Recursively find all reachable code
        self.find_reachable_code(program);

        // Step 3: Remove unreachable code from program
        self.remove_unreachable_code(program);
    }

    /// Mark entry points (main function, exported functions, etc.)
    fn mark_entry_points(&mut self, program: &Program) {
        // Always keep the main function
        for fn_def in &program.functions {
            if fn_def.name == "main" {
                self.reachable_functions.insert(fn_def.name.clone());
            }
        }

        // Keep all public functions
        for fn_def in &program.functions {
            if fn_def.visibility == crate::ast::Visibility::Public {
                self.reachable_functions.insert(fn_def.name.clone());
            }
        }

        // Keep public structs
        for struct_def in &program.structs {
            if struct_def.visibility == crate::ast::Visibility::Public {
                self.reachable_structs.insert(struct_def.name.clone());
            }
        }

        // Keep public enums
        for enum_def in &program.enums {
            if enum_def.visibility == crate::ast::Visibility::Public {
                self.reachable_enums.insert(enum_def.name.clone());
            }
        }
    }

    /// Find all reachable code by traversing from entry points
    fn find_reachable_code(&mut self, program: &Program) {
        // Keep iterating until no new symbols are added
        loop {
            let mut changed = false;

            // Check functions
            for fn_def in &program.functions {
                if self.reachable_functions.contains(&fn_def.name) {
                    // Mark the function as reachable, but also check for generic instantiations
                    if !fn_def.generic_params.is_empty() {
                        // This is a generic function, we'll handle its usages during traversal
                    }
                    changed |= self.traverse_fn(fn_def);
                }
            }

            // Check struct methods - if struct is reachable, its methods should be reachable too
            for struct_def in &program.structs {
                if self.reachable_structs.contains(&struct_def.name) {
                    for method in &struct_def.methods {
                        let method_name = format!("{}_{}", struct_def.name, method.name);
                        if !self.reachable_functions.contains(&method_name) {
                            self.reachable_functions.insert(method_name.clone());
                            changed |= self.traverse_fn(method);
                        }
                    }
                }
            }

            // Check enum methods - if enum is reachable, its methods should be reachable too
            for enum_def in &program.enums {
                if self.reachable_enums.contains(&enum_def.name) {
                    for method in &enum_def.methods {
                        let method_name = format!("{}_{}", enum_def.name, method.name);
                        if !self.reachable_functions.contains(&method_name) {
                            self.reachable_functions.insert(method_name.clone());
                            changed |= self.traverse_fn(method);
                        }
                    }
                }
            }

            if !changed {
                break;
            }
        }
    }

    /// Traverse a function definition to find reachable symbols
    fn traverse_fn(&mut self, fn_def: &FnDef) -> bool {
        let mut changed = false;

        // Check function body statements
        for stmt in &fn_def.body {
            changed |= self.traverse_stmt(stmt);
        }

        // Check parameter and return types
        for param in &fn_def.params {
            changed |= self.traverse_type(&param.ty);
        }
        changed |= self.traverse_type(&fn_def.return_ty);

        changed
    }

    /// Traverse a statement to find reachable symbols
    fn traverse_stmt(&mut self, stmt: &Stmt) -> bool {
        let mut changed = false;
        match stmt {
            Stmt::Expr { expr, .. } => {
                changed = self.traverse_expr(expr);
            }
            Stmt::Return { value, .. } => {
                if let Some(expr) = value {
                    changed = self.traverse_expr(expr);
                }
            }
            Stmt::Let { ty, value, .. } => {
                if let Some(t) = ty {
                    changed = self.traverse_type(t);
                }
                if let Some(val) = value {
                    changed |= self.traverse_expr(val);
                }
            }
            Stmt::Assign { target, value, .. } => {
                // Check if target is a struct field access
                if let Some(dot_pos) = target.find('.') {
                    let struct_name = &target[..dot_pos];
                    // Mark struct as reachable if it's a known type
                    if self.symbol_table.resolve(struct_name).is_some() {
                        self.reachable_structs.insert(struct_name.to_string());
                        changed = true;
                    }
                }
                changed |= self.traverse_expr(value);
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                changed = self.traverse_expr(condition);
                changed |= self.traverse_stmt(then_branch);
                if let Some(else_b) = else_branch {
                    changed |= self.traverse_stmt(else_b);
                }
            }
            Stmt::For { iterable, body, .. } => {
                changed = self.traverse_expr(iterable);
                changed |= self.traverse_stmt(body);
            }
            Stmt::Switch {
                condition, cases, ..
            } => {
                changed = self.traverse_expr(condition);
                for case in cases {
                    changed |= self.traverse_stmt(&case.body);
                }
            }
            Stmt::Break { .. } => {}
            Stmt::Defer { stmt, .. } => {
                changed = self.traverse_stmt(stmt);
            }
            Stmt::Block { stmts, .. } => {
                for s in stmts {
                    changed |= self.traverse_stmt(s);
                }
            }
            Stmt::DeferBang { stmt, .. } => {
                changed = self.traverse_stmt(stmt);
            }
            Stmt::Import { .. } => {}
        }
        changed
    }

    /// Traverse an expression to find reachable symbols
    fn traverse_expr(&mut self, expr: &Expr) -> bool {
        let mut changed = false;
        match expr {
            Expr::Int(_, _)
            | Expr::Float(_, _)
            | Expr::Bool(_, _)
            | Expr::String(_, _)
            | Expr::Char(_, _) => {}
            Expr::Null(_) => {}
            Expr::Ident(name, _) => {
                // Check if this is a known struct/enum type
                if self.symbol_table.resolve(name).is_some() {
                    // Could be a type reference
                    // Don't add to functions
                }
            }
            Expr::Binary { left, right, .. } => {
                changed = self.traverse_expr(left);
                changed |= self.traverse_expr(right);
            }
            Expr::Unary { expr, .. } => {
                changed = self.traverse_expr(expr);
            }
            Expr::Call {
                name,
                args,
                generic_args,
                ..
            } => {
                // Handle generic function calls like add<T>(a, b)
                let fn_name = name.clone();

                // Mark the function as reachable
                if !self.reachable_functions.contains(&fn_name) {
                    self.reachable_functions.insert(fn_name.clone());
                    changed = true;
                }

                // If there are generic type arguments, track them
                if !generic_args.is_empty() {
                    let type_args: Vec<String> =
                        generic_args.iter().map(|t| format!("{:?}", t)).collect();
                    let generic_key = format!("{}<{}>", fn_name, type_args.join(", "));
                    if !self.reachable_generic_functions.contains(&generic_key) {
                        self.reachable_generic_functions.insert(generic_key);
                        changed = true;
                    }
                }

                // Traverse arguments
                for arg in args {
                    changed |= self.traverse_expr(arg);
                }
            }
            Expr::MemberAccess { object, member, .. } => {
                changed = self.traverse_expr(object);
                // If this is a method call (like obj.method()), we'll handle it
                // in the Call expression
                let _ = member; // suppress unused warning
            }
            Expr::Tuple(vals, _) => {
                for val in vals {
                    changed |= self.traverse_expr(val);
                }
            }
            Expr::TupleIndex { tuple, .. } => {
                changed = self.traverse_expr(tuple);
            }
            Expr::Array(vals, _, _) => {
                for val in vals {
                    changed |= self.traverse_expr(val);
                }
            }
            Expr::Struct {
                name,
                fields,
                generic_args,
                ..
            } => {
                // Handle generic struct instantiation like Compose<T>{...}
                let struct_name = name.clone();

                // Mark struct as reachable
                if !self.reachable_structs.contains(&struct_name) {
                    self.reachable_structs.insert(struct_name.clone());
                    changed = true;
                }

                // Track generic instantiation
                if !generic_args.is_empty() {
                    let type_args: Vec<String> =
                        generic_args.iter().map(|t| format!("{:?}", t)).collect();
                    let instantiation_key = format!("{}<{}>", struct_name, type_args.join(", "));
                    if !self
                        .reachable_generic_instantiations
                        .contains(&instantiation_key)
                    {
                        self.reachable_generic_instantiations
                            .insert(instantiation_key);
                        changed = true;
                    }
                }

                // Traverse field values
                for (_, val) in fields {
                    changed |= self.traverse_expr(val);
                }
            }
            Expr::Cast {
                expr, target_type, ..
            } => {
                changed = self.traverse_expr(expr);
                changed |= self.traverse_type(target_type);
            }
            Expr::Try { expr, .. } => {
                changed = self.traverse_expr(expr);
            }
            Expr::Catch { expr, body, .. } => {
                changed = self.traverse_expr(expr);
                changed |= self.traverse_expr(body);
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                changed = self.traverse_expr(condition);
                changed |= self.traverse_expr(then_branch);
                changed |= self.traverse_expr(else_branch);
            }
            Expr::Block { stmts, .. } => {
                for s in stmts {
                    changed |= self.traverse_stmt(s);
                }
            }
        }
        changed
    }

    /// Traverse a type to find reachable custom types
    fn traverse_type(&mut self, ty: &Type) -> bool {
        let mut changed = false;
        match ty {
            Type::Custom {
                name, generic_args, ..
            } => {
                // Mark the custom type as reachable
                if !self.reachable_structs.contains(name) && !self.reachable_enums.contains(name) {
                    // Check if it's an enum
                    if self.symbol_table.resolve(name).is_some() {
                        // Could be either, mark as potential struct
                        self.reachable_structs.insert(name.clone());
                        changed = true;
                    }
                }
                // Traverse generic arguments
                for arg in generic_args {
                    changed |= self.traverse_type(arg);
                }
            }
            Type::Pointer(inner) | Type::Option(inner) | Type::Result(inner) => {
                changed = self.traverse_type(inner);
            }
            Type::Array { element_type, .. } => {
                changed = self.traverse_type(element_type);
            }
            Type::Tuple(types) => {
                for t in types {
                    changed |= self.traverse_type(t);
                }
            }
            Type::Function {
                params,
                return_type,
            } => {
                for param in params {
                    changed |= self.traverse_type(param);
                }
                changed |= self.traverse_type(return_type);
            }
            Type::GenericParam(_) => {
                // Generic parameters are always reachable when used
            }
            _ => {}
        }
        changed
    }

    /// Remove unreachable code from the program
    fn remove_unreachable_code(&mut self, program: &mut Program) {
        // Filter functions
        program.functions.retain(|f| {
            self.reachable_functions.contains(&f.name)
                || f.visibility == crate::ast::Visibility::Public
        });

        // Filter structs - need to filter methods too
        program.structs.retain(|s| {
            self.reachable_structs.contains(&s.name)
                || s.visibility == crate::ast::Visibility::Public
        });

        // Filter enums - need to filter methods too
        program.enums.retain(|e| {
            self.reachable_enums.contains(&e.name) || e.visibility == crate::ast::Visibility::Public
        });
    }

    /// Get statistics about what was removed
    pub fn get_stats(&self) -> TreeShakerStats {
        TreeShakerStats {
            reachable_functions: self.reachable_functions.len(),
            reachable_structs: self.reachable_structs.len(),
            reachable_enums: self.reachable_enums.len(),
            reachable_generic_instantiations: self.reachable_generic_instantiations.len(),
        }
    }
}

/// Statistics about tree-shaking results
#[derive(Debug)]
pub struct TreeShakerStats {
    pub reachable_functions: usize,
    pub reachable_structs: usize,
    pub reachable_enums: usize,
    pub reachable_generic_instantiations: usize,
}

/// Run tree-shaking on a program
pub fn treeshake(program: &mut Program, symbol_table: SymbolTable) -> TreeShakerStats {
    let mut shaker = TreeShaker::new(symbol_table);
    shaker.shake(program);
    shaker.get_stats()
}
