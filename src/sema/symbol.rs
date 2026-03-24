use crate::ast::{Type, Visibility};
use std::collections::HashMap;

#[derive(Debug, Clone)]
#[allow(unused)]
pub struct Symbol {
    pub name: String,
    pub ty: Type,
    pub visibility: Visibility,
    pub is_const: bool,
    /// Generic type parameters (for functions or structs)
    pub generic_params: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Scope {
    pub symbols: HashMap<String, Symbol>,
    pub parent: Option<usize>,
}

#[derive(Debug, Clone)]
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

    pub fn define(
        &mut self,
        name: String,
        ty: Type,
        visibility: Visibility,
        is_const: bool,
    ) {
        self.define_with_generics(name, ty, visibility, is_const, Vec::new());
    }

    pub fn define_with_generics(
        &mut self,
        name: String,
        ty: Type,
        visibility: Visibility,
        is_const: bool,
        generic_params: Vec<String>,
    ) {
        let symbol = Symbol {
            name: name.clone(),
            ty,
            visibility,
            is_const,
            generic_params,
        };
        self.scopes[self.current_scope].symbols.insert(name, symbol);
    }

    /// Check if a symbol already exists in the current scope
    pub fn contains(&self, name: &str) -> bool {
        self.scopes[self.current_scope].symbols.contains_key(name)
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

    /// Merge another symbol table's global scope into this one
    pub fn merge(&mut self, other: SymbolTable) {
        // Merge global scope (index 0) symbols from other table
        if let Some(global_scope) = other.scopes.first() {
            for (name, symbol) in &global_scope.symbols {
                // Only add if not already defined in current scope
                if !self.scopes[self.current_scope].symbols.contains_key(name) {
                    self.scopes[self.current_scope]
                        .symbols
                        .insert(name.clone(), symbol.clone());
                }
            }
        }
    }
}
