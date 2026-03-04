use std::collections::HashMap;
use crate::ast::{Type, Visibility};

#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub ty: Type,
    pub visibility: Visibility,
    pub is_const: bool,
}

pub struct Scope {
    pub symbols: HashMap<String, Symbol>,
    pub parent: Option<usize>,
}

pub struct SymbolTable {
    pub scopes: Vec<Scope>,
    pub current_scope: usize,
}

impl SymbolTable {
    pub fn new() -> Self {
        let global_scope = Scope {
            symbols: HashMap::new(),
            parent: None,
        };
        SymbolTable {
            scopes: vec![global_scope],
            current_scope: 0,
        }
    }

    pub fn enter_scope(&mut self) {
        let new_scope = Scope {
            symbols: HashMap::new(),
            parent: Some(self.current_scope),
        };
        self.scopes.push(new_scope);
        self.current_scope = self.scopes.len() - 1;
    }

    pub fn exit_scope(&mut self) {
        if let Some(parent) = self.scopes[self.current_scope].parent {
            self.current_scope = parent;
        }
    }

    pub fn define(&mut self, name: String, ty: Type, visibility: Visibility, is_const: bool) {
        let symbol = Symbol {
            name: name.clone(),
            ty,
            visibility,
            is_const,
        };
        self.scopes[self.current_scope].symbols.insert(name, symbol);
    }

    pub fn resolve(&self, name: &str) -> Option<&Symbol> {
        let mut scope_idx = Some(self.current_scope);
        while let Some(idx) = scope_idx {
            let scope = &self.scopes[idx];
            if let Some(symbol) = scope.symbols.get(name) {
                return Some(symbol);
            }
            scope_idx = scope.parent;
        }
        None
    }
}

pub struct SemanticAnalyzer {
    pub symbol_table: SymbolTable,
}

impl SemanticAnalyzer {
    pub fn new() -> Self {
        SemanticAnalyzer {
            symbol_table: SymbolTable::new(),
        }
    }

    pub fn analyze(&mut self, program: &crate::ast::Program) -> Result<(), String> {
        // Pass 1: Collect globals (functions, structs, enums)
        self.collect_globals(program)?;

        // Pass 2: Analyze function bodies
        for f in &program.functions {
            self.analyze_fn(f)?;
        }

        Ok(())
    }

    fn collect_globals(&mut self, program: &crate::ast::Program) -> Result<(), String> {
        // Add functions
        for f in &program.functions {
            // Check for duplicates in global scope
            if self.symbol_table.resolve(&f.name).is_some() {
                return Err(format!("Duplicate declaration of function '{}'", f.name));
            }
            // For now, simplify function representation in symbol table
            self.symbol_table.define(f.name.clone(), f.return_ty.clone().unwrap_or(Type::Void), f.visibility, true);
        }

        // Add structs
        for s in &program.structs {
            if self.symbol_table.resolve(&s.name).is_some() {
                return Err(format!("Duplicate declaration of type '{}'", s.name));
            }
            self.symbol_table.define(s.name.clone(), Type::Custom {
                name: s.name.clone(),
                generic_args: vec![],
                is_exported: s.visibility.is_public(),
            }, s.visibility, true);
        }

        // Add enums
        for e in &program.enums {
            if self.symbol_table.resolve(&e.name).is_some() {
                return Err(format!("Duplicate declaration of type '{}'", e.name));
            }
            self.symbol_table.define(e.name.clone(), Type::Custom {
                name: e.name.clone(),
                generic_args: vec![],
                is_exported: e.visibility.is_public(),
            }, e.visibility, true);
        }

        Ok(())
    }

    fn analyze_fn(&mut self, f: &crate::ast::FnDef) -> Result<(), String> {
        self.symbol_table.enter_scope();

        // Define parameters
        for param in &f.params {
            self.symbol_table.define(param.name.clone(), param.ty.clone(), Visibility::Private, false);
        }

        // Analyze function body
        for stmt in &f.body {
            self.analyze_stmt(stmt)?;
        }

        self.symbol_table.exit_scope();
        Ok(())
    }

    fn analyze_stmt(&mut self, stmt: &crate::ast::Stmt) -> Result<(), String> {
        match stmt {
            crate::ast::Stmt::Expr { expr, .. } => {
                self.analyze_expr(expr)?;
            }
            crate::ast::Stmt::Let { name, ty, value, mutability, .. } => {
                let inferred_ty = if let Some(val_expr) = value {
                    let v_ty = self.analyze_expr(val_expr)?;
                    if let Some(explicit_ty) = ty {
                        if !self.types_compatible(explicit_ty, &v_ty) {
                            return Err(format!("Type mismatch in variable '{}' declaration: expected {}, found {}", name, explicit_ty, v_ty));
                        }
                    }
                    v_ty
                } else if let Some(explicit_ty) = ty {
                    explicit_ty.clone()
                } else {
                    return Err(format!("Variable '{}' must have either a type or an initial value", name));
                };

                self.symbol_table.define(name.clone(), inferred_ty, Visibility::Private, matches!(mutability, crate::ast::Mutability::Const));
            }
            crate::ast::Stmt::Assign { target, value, .. } => {
                let (symbol_ty, is_const) = {
                    let symbol = self.symbol_table.resolve(target)
                        .ok_or_else(|| format!("Undefined variable '{}'", target))?;
                    (symbol.ty.clone(), symbol.is_const)
                };
                
                if is_const {
                    return Err(format!("Cannot reassign constant variable '{}'", target));
                }

                let expr_ty = self.analyze_expr(value)?;
                if !self.types_compatible(&symbol_ty, &expr_ty) {
                    return Err(format!("Type mismatch in assignment to '{}': expected {}, found {}", target, symbol_ty, expr_ty));
                }
            }
            crate::ast::Stmt::Return { value, .. } => {
                if let Some(val_expr) = value {
                    self.analyze_expr(val_expr)?;
                    // TODO: Check against function return type
                }
            }
            crate::ast::Stmt::If { condition, then_branch, else_branch, .. } => {
                let cond_ty = self.analyze_expr(condition)?;
                if cond_ty != Type::Bool && cond_ty != Type::I64 { // Allow i64 for simplicity if needed
                     // return Err(format!("'if' condition must be boolean or integer, found {}", cond_ty));
                }
                self.analyze_stmt(then_branch)?;
                if let Some(eb) = else_branch {
                    self.analyze_stmt(eb)?;
                }
            }
            crate::ast::Stmt::Block { stmts, .. } => {
                self.symbol_table.enter_scope();
                for s in stmts {
                    self.analyze_stmt(s)?;
                }
                self.symbol_table.exit_scope();
            }
            crate::ast::Stmt::While { condition, body, .. } => {
                self.analyze_expr(condition)?;
                self.analyze_stmt(body)?;
            }
            _ => {
                // TODO: Implement other statements
            }
        }
        Ok(())
    }

    fn analyze_expr(&mut self, expr: &crate::ast::Expr) -> Result<Type, String> {
        match expr {
            crate::ast::Expr::Int(_, _) => Ok(Type::I64),
            crate::ast::Expr::Bool(_, _) => Ok(Type::Bool),
            crate::ast::Expr::String(_, _) => Ok(Type::Custom { 
                name: "String".to_string(), 
                generic_args: vec![], 
                is_exported: false 
            }),
            crate::ast::Expr::Char(_, _) => Ok(Type::I8),
            crate::ast::Expr::Ident(name, _) => {
                self.symbol_table.resolve(name)
                    .map(|s| s.ty.clone())
                    .ok_or_else(|| format!("Undefined variable '{}'", name))
            }
            crate::ast::Expr::Binary { op, left, right, .. } => {
                let l_ty = self.analyze_expr(left)?;
                let r_ty = self.analyze_expr(right)?;

                match op {
                    crate::ast::BinaryOp::Add
                    | crate::ast::BinaryOp::Sub
                    | crate::ast::BinaryOp::Mul
                    | crate::ast::BinaryOp::Div
                    | crate::ast::BinaryOp::Mod
                    | crate::ast::BinaryOp::BitAnd
                    | crate::ast::BinaryOp::BitOr
                    | crate::ast::BinaryOp::BitXor
                    | crate::ast::BinaryOp::Shl
                    | crate::ast::BinaryOp::Shr => {
                        if self.is_numeric(&l_ty) && self.is_numeric(&r_ty) {
                            Ok(l_ty) // Result is same as left operand for now
                        } else {
                            Err(format!(
                                "Binary operation {:?} requires numeric operands, found {} and {}",
                                op, l_ty, r_ty
                            ))
                        }
                    }
                    crate::ast::BinaryOp::Eq
                    | crate::ast::BinaryOp::Ne
                    | crate::ast::BinaryOp::Lt
                    | crate::ast::BinaryOp::Gt
                    | crate::ast::BinaryOp::Le
                    | crate::ast::BinaryOp::Ge => {
                        if self.types_compatible(&l_ty, &r_ty) {
                            Ok(Type::Bool)
                        } else {
                            Err(format!(
                                "Comparison requires compatible types, found {} and {}",
                                l_ty, r_ty
                            ))
                        }
                    }
                    crate::ast::BinaryOp::And | crate::ast::BinaryOp::Or => {
                        if l_ty == Type::Bool && r_ty == Type::Bool {
                            Ok(Type::Bool)
                        } else {
                            Err(format!(
                                "Logical operation requires boolean operands, found {} and {}",
                                l_ty, r_ty
                            ))
                        }
                    }
                    crate::ast::BinaryOp::Range => Ok(Type::I64),
                }
            }
            crate::ast::Expr::Unary { op, expr, .. } => {
                let e_ty = self.analyze_expr(expr)?;
                match op {
                    crate::ast::UnaryOp::Neg | crate::ast::UnaryOp::Pos => {
                        if self.is_numeric(&e_ty) {
                            Ok(e_ty)
                        } else {
                            Err(format!("Unary {:?} requires numeric operand, found {}", op, e_ty))
                        }
                    }
                    crate::ast::UnaryOp::Not => {
                        if e_ty == Type::Bool {
                            Ok(Type::Bool)
                        } else {
                            Err(format!("Logical NOT requires boolean operand, found {}", e_ty))
                        }
                    }
                }
            }
            crate::ast::Expr::Call { name, namespace, args, .. } => {
                // If namespace is IO, skip for now or resolve from stdlib
                if namespace.as_deref() == Some("io") && name == "println" {
                    for arg in args {
                        self.analyze_expr(arg)?;
                    }
                    return Ok(Type::Void);
                }

                let symbol_ty = if namespace.is_some() {
                    Type::I64 // Default return type for external functions
                } else {
                    self.symbol_table.resolve(name)
                        .map(|s| s.ty.clone())
                        .ok_or_else(|| format!("Undefined function '{}'", name))?
                };
                
                for arg in args {
                    self.analyze_expr(arg)?;
                }
                
                Ok(symbol_ty)
            }
            _ => Ok(Type::I64), // Placeholder for other expressions
        }
    }

    fn is_numeric(&self, ty: &Type) -> bool {
        matches!(ty, Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::U8 | Type::U16 | Type::U32 | Type::U64)
    }

    fn types_compatible(&self, left: &Type, right: &Type) -> bool {
        // Simple equality check for now
        left == right || (self.is_numeric(left) && self.is_numeric(right))
    }
}
