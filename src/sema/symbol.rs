use crate::ast::{Type, Visibility};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum ConstantValue {
    Int(i64),
    Float(f64),
    Bool(bool),
}

#[derive(Debug, Clone)]
#[allow(unused)]
pub struct Symbol {
    pub name: String,
    pub ty: Type,
    pub visibility: Visibility,
    pub is_const: bool,
    pub const_value: Option<ConstantValue>,
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
        self.define_with_generics(name, ty, visibility, is_const, Vec::new(), None);
    }

    pub fn define_with_value(
        &mut self,
        name: String,
        ty: Type,
        visibility: Visibility,
        is_const: bool,
        value: Option<ConstantValue>,
    ) {
        self.define_with_generics(name, ty, visibility, is_const, Vec::new(), value);
    }

    pub fn define_with_generics(
        &mut self,
        name: String,
        ty: Type,
        visibility: Visibility,
        is_const: bool,
        generic_params: Vec<String>,
        const_value: Option<ConstantValue>,
    ) {
        let symbol = Symbol {
            name: name.clone(),
            ty,
            visibility,
            is_const,
            const_value,
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

    pub fn resolve_mut(&mut self, name: &str) -> Option<&mut Symbol> {
        let mut scope_idx = Some(self.current_scope);
        while let Some(idx) = scope_idx {
            // We need to work around the borrow checker here.
            // If the symbol is found, we return it. If not, we move to the parent.
            if self.scopes[idx].symbols.contains_key(name) {
                return self.scopes[idx].symbols.get_mut(name);
            }
            scope_idx = self.scopes[idx].parent;
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
