use crate::ast::Visibility;
use crate::sema::error::{AnalysisError, AnalysisResult};
use crate::sema::infer::TypedProgram;
use crate::sema::symbol::SymbolTable;

// ============================================================================
// Analysis Pass 4: Mutability Analyzer
// Checks that const variables aren't reassigned
// ============================================================================

pub struct MutabilityAnalyzer {
    symbol_table: SymbolTable,
    #[allow(unused)]
    typed_program: TypedProgram,
}

impl MutabilityAnalyzer {
    pub fn new(symbol_table: SymbolTable, typed_program: TypedProgram) -> Self {
        MutabilityAnalyzer {
            symbol_table,
            typed_program,
        }
    }

    pub fn analyze(&mut self, program: &crate::ast::Program) -> AnalysisResult<()> {
        for f in &program.functions {
            if f.generic_params.is_empty() {
                self.analyze_function(f, program)?;
            }
        }
        Ok(())
    }

    fn analyze_function(
        &mut self,
        f: &crate::ast::FnDef,
        program: &crate::ast::Program,
    ) -> AnalysisResult<()> {
        self.symbol_table.enter_scope();
        for param in &f.params {
            self.symbol_table.define(
                param.name.clone(),
                param.ty.clone(),
                Visibility::Private,
                false, // Parameters are mutable by default in this language
            );
        }
        for stmt in &f.body {
            self.analyze_statement(stmt, program)?;
        }
        self.symbol_table.exit_scope();
        Ok(())
    }

    fn analyze_statement(
        &mut self,
        stmt: &crate::ast::Stmt,
        program: &crate::ast::Program,
    ) -> AnalysisResult<()> {
        match stmt {
            crate::ast::Stmt::Assign {
                target,
                value: _,
                op: _,
                span,
            } => {
                if target != "_" {
                    // Improved target parsing to handle both . and []
                    let mut parts = Vec::new();
                    let mut current_part = String::new();
                    let mut chars = target.chars().peekable();

                    while let Some(c) = chars.next() {
                        match c {
                            '.' => {
                                if !current_part.is_empty() {
                                    parts.push(current_part.clone());
                                    current_part.clear();
                                }
                                if chars.peek() == Some(&'*') {
                                    chars.next();
                                    parts.push("*".to_string());
                                }
                            }
                            '[' => {
                                if !current_part.is_empty() {
                                    parts.push(current_part.clone());
                                    current_part.clear();
                                }
                                let mut index_str = String::new();
                                while let Some(&nc) = chars.peek() {
                                    if nc == ']' {
                                        chars.next();
                                        break;
                                    }
                                    index_str.push(chars.next().unwrap());
                                }
                                parts.push(format!("[{}]", index_str));
                            }
                            _ => {
                                current_part.push(c);
                            }
                        }
                    }
                    if !current_part.is_empty() {
                        parts.push(current_part);
                    }

                    let base = &parts[0];

                    if let Some(symbol) = self.symbol_table.resolve(base) {
                        let mut current_is_const = symbol.is_const;
                        let mut current_ty = &symbol.ty;

                        // If we only have the base name (no .member or .*)
                        if parts.len() == 1 {
                            if current_is_const {
                                return Err(AnalysisError::new_with_span(
                                    &format!("Cannot reassign constant variable '{}'", base),
                                    span,
                                )
                                .with_module("mutability"));
                            }
                        } else {
                            // Check nested components
                            for i in 1..parts.len() {
                                let part = &parts[i];

                                if part == "*" {
                                    // Dereference: check if pointer points to const
                                    match current_ty {
                                        crate::ast::Type::Pointer(inner) => {
                                            current_ty = inner.as_ref();
                                            // The pointer itself being const doesn't prevent modifying pointee
                                            // But if the pointee type is Const(T), it's blocked.
                                            current_is_const =
                                                matches!(current_ty, crate::ast::Type::Const(_));
                                            if let crate::ast::Type::Const(inner_inner) = current_ty
                                            {
                                                current_ty = inner_inner.as_ref();
                                            }
                                        }
                                        _ => break,
                                    }
                                } else if part.starts_with('[') && part.ends_with(']') {
                                    // Indexing
                                    if current_is_const {
                                        return Err(AnalysisError::new_with_span(
                                            &format!(
                                                "Cannot modify constant elements of '{}'",
                                                base
                                            ),
                                            span,
                                        )
                                        .with_module("mutability"));
                                    }

                                    if let crate::ast::Type::Array { element_type, .. } = current_ty
                                    {
                                        current_ty = element_type.as_ref();
                                        if let crate::ast::Type::Const(inner) = current_ty {
                                            current_is_const = true;
                                            current_ty = inner.as_ref();
                                        }
                                    }
                                } else {
                                    // Member access (struct field)
                                    if current_is_const {
                                        return Err(AnalysisError::new_with_span(
                                            &format!("Cannot modify field '{}' of constant variable '{}'", part, base),
                                            span,
                                        )
                                        .with_module("mutability"));
                                    }
                                    // We don't update current_ty here as we don't have easy access to struct field types in this pass.
                                }
                            }
                        }
                    }
                }
                Ok(())
            }
            crate::ast::Stmt::Block { stmts, .. } => {
                self.symbol_table.enter_scope();
                for s in stmts {
                    self.analyze_statement(s, program)?;
                }
                self.symbol_table.exit_scope();
                Ok(())
            }
            crate::ast::Stmt::If {
                then_branch,
                else_branch,
                capture,
                ..
            } => {
                let cap = capture.clone();
                if cap.is_some() {
                    self.symbol_table.enter_scope();
                    self.symbol_table.define(
                        cap.unwrap(),
                        crate::ast::Type::I64,
                        Visibility::Private,
                        false,
                    );
                    self.analyze_statement(then_branch, program)?;
                    self.symbol_table.exit_scope();
                } else {
                    self.analyze_statement(then_branch, program)?;
                }
                if let Some(eb) = else_branch {
                    self.analyze_statement(eb, program)?;
                }
                Ok(())
            }
            crate::ast::Stmt::For {
                var_name,
                iterable: _,
                capture,
                index_var,
                body,
                ..
            } => {
                self.symbol_table.enter_scope();
                if let Some(vn) = var_name {
                    self.symbol_table.define(
                        vn.clone(),
                        crate::ast::Type::I64,
                        Visibility::Private,
                        false,
                    );
                }
                if let Some(cv) = capture {
                    self.symbol_table.define(
                        cv.clone(),
                        crate::ast::Type::I64,
                        Visibility::Private,
                        false,
                    );
                }
                if let Some(iv) = index_var {
                    self.symbol_table.define(
                        iv.clone(),
                        crate::ast::Type::I64,
                        Visibility::Private,
                        false,
                    );
                }
                self.analyze_statement(body, program)?;
                self.symbol_table.exit_scope();
                Ok(())
            }
            crate::ast::Stmt::Switch { cases, .. } => {
                for case in cases {
                    self.analyze_statement(&case.body, program)?;
                }
                Ok(())
            }
            crate::ast::Stmt::Defer { stmt, .. } => {
                self.analyze_statement(stmt, program)?;
                Ok(())
            }
            crate::ast::Stmt::Let {
                name,
                names,
                ty,
                value,
                mutability,
                visibility,
                span: _,
            } => {
                let is_const = *mutability == crate::ast::Mutability::Const;
                // Use I64 as default type when no type annotation and no value
                let default_ty = crate::ast::Type::I64;
                let inferred_ty = ty.clone().unwrap_or(default_ty);

                // Handle tuple destructuring
                if let Some(var_names) = names {
                    for var_name in var_names {
                        if let Some(n) = var_name {
                            self.symbol_table.define(
                                n.clone(),
                                inferred_ty.clone(),
                                *visibility,
                                is_const,
                            );
                        }
                    }
                } else {
                    self.symbol_table
                        .define(name.clone(), inferred_ty, *visibility, is_const);
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    pub fn get_symbol_table(&self) -> &SymbolTable {
        &self.symbol_table
    }
}
