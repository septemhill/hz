use super::*;

#[allow(unused)]
impl<'ctx> CodeGenerator<'ctx> {
    pub(crate) fn generate_hir_stmt(&mut self, stmt: &hir::HirStmt) -> CodegenResult<()> {
        match stmt {
            hir::HirStmt::Expr(expr) => {
                self.generate_hir_expr(expr)?;
                Ok(())
            }
            hir::HirStmt::Let {
                name,
                ty,
                value,
                mutability,
                ..
            } => {
                let llvm_type = self.llvm_type(ty);
                let alloca = self.builder.build_alloca(llvm_type, name)?;
                if let Some(val) = value {
                    // Check if we need special handling for identifiers that might be functions
                    let mut llvm_val = if let hir::HirExpr::Ident(ident_name, _, _) = val {
                        // Try to look up as a function first
                        let fn_name = if ident_name == "main" {
                            "main".to_string()
                        } else {
                            format!("{}_{}", self.module_name, ident_name)
                        };

                        if self.module.get_function(&fn_name).is_some() {
                            eprintln!("DEBUG: Let - identifier is a function: {}", fn_name);
                            // This identifier is a function - get the function pointer
                            if let Some(fn_val) = self.module.get_function(&fn_name) {
                                fn_val.as_global_value().as_pointer_value().into()
                            } else {
                                self.generate_hir_expr(val)?
                            }
                        } else {
                            // Not a function, process normally
                            self.generate_hir_expr(val)?
                        }
                    } else {
                        self.generate_hir_expr(val)?
                    };
                    llvm_val = self.coerce_type(llvm_val, ty)?;
                    self.builder.build_store(alloca, llvm_val)?;
                }
                self.variables.insert(name.clone(), alloca);
                self.variable_types.insert(name.clone(), ty.clone());
                if *mutability == Mutability::Const {
                    self.const_variables.insert(name.clone(), alloca);
                }
                Ok(())
            }
            hir::HirStmt::Assign { target, value, .. } => {
                // Skip underscore assignment (used for ignoring values)
                if target == "_" {
                    let _llvm_val = self.generate_hir_expr(value)?;
                    // Just evaluate and discard
                    return Ok(());
                }

                if target.contains('.') {
                    let parts: Vec<&str> = target.split('.').collect();
                    let obj_name = parts[0];
                    let member = parts[1];

                    let ptr = self.variables.get(obj_name).ok_or("Var not found")?.clone();
                    let var_ty = self.variable_types.get(obj_name).unwrap();

                    // First, get the struct name from the variable type
                    let struct_name = match var_ty {
                        Type::Pointer(inner) => {
                            if let Type::Custom { name, .. } = &**inner {
                                name.clone()
                            } else {
                                return Err("Member assignment on non-custom pointer type".into());
                            }
                        }
                        Type::Custom { name, .. } => name.clone(),
                        _ => return Err("Member assignment on non-struct type".into()),
                    };

                    // Look up the field index from our mapping
                    let field_idx = self
                        .struct_field_indices
                        .get(&struct_name)
                        .and_then(|fields| fields.get(member))
                        .copied()
                        .ok_or_else(|| {
                            format!("Field '{}' not found in struct '{}'", member, struct_name)
                        })?;

                    // Determine struct_ptr and struct_type based on whether the variable is a pointer to a struct or the struct itself
                    let (struct_ptr, struct_type) = if let Type::Pointer(inner) = var_ty {
                        if let Type::Custom {
                            name: struct_name, ..
                        } = &**inner
                        {
                            let st =
                                self.context
                                    .get_struct_type(struct_name)
                                    .unwrap_or_else(|| {
                                        panic!("Struct lookup failed for: {}", struct_name)
                                    });
                            // The alloca holds a pointer to the struct. Load it using the generic pointer type.
                            let opaque_ptr_type =
                                self.context.ptr_type(inkwell::AddressSpace::default());
                            let loaded_ptr = self
                                .builder
                                .build_load(opaque_ptr_type, ptr, "obj_ptr")?
                                .into_pointer_value();
                            (loaded_ptr, st)
                        } else {
                            return Err("Member assignment on non-custom pointer type".into());
                        }
                    } else if let Type::Custom {
                        name: struct_name, ..
                    } = var_ty
                    {
                        let st = self
                            .context
                            .get_struct_type(struct_name)
                            .unwrap_or_else(|| panic!("Struct lookup failed for: {}", struct_name));
                        // ptr is alloca to struct
                        (ptr, st)
                    } else {
                        return Err("Member assignment on non-struct type".into());
                    };

                    let field_ptr = self
                        .builder
                        .build_struct_gep(struct_type, struct_ptr, field_idx, "field_ptr")
                        .map_err(|e| e.to_string())?;

                    // Get the exact field type to coerce the assigned value
                    let field_type = struct_type.get_field_type_at_index(field_idx).unwrap();
                    let mut llvm_val = self.generate_hir_expr(value)?;

                    // Simple int casting for assignment coercion if needed
                    if field_type.is_int_type() && llvm_val.is_int_value() {
                        llvm_val = self
                            .builder
                            .build_int_cast(
                                llvm_val.into_int_value(),
                                field_type.into_int_type(),
                                "assign_cast",
                            )?
                            .into();
                    }

                    self.builder.build_store(field_ptr, llvm_val)?;
                    return Ok(());
                }

                let ptr = self.variables.get(target).ok_or("Var not found")?.clone();
                let expected_t = self
                    .variable_types
                    .get(target)
                    .ok_or("Var type not found")?
                    .clone();
                let mut llvm_val = self.generate_hir_expr(value)?;
                llvm_val = self.coerce_type(llvm_val, &expected_t)?;
                self.builder.build_store(ptr, llvm_val)?;
                Ok(())
            }
            hir::HirStmt::Return(value, _) => {
                // Execute defers before returning
                self.pop_defer_scope()?;

                let is_main = self.current_function_is_main();
                let ret_ty = self
                    .return_type
                    .clone()
                    .ok_or("Return encountered without current function return type")?;

                if let Some(val) = value {
                    let mut llvm_val = self.generate_hir_expr(val)?;

                    // Skip coercion for error type expressions - they already return i64
                    let is_error_expr = match val {
                        hir::HirExpr::MemberAccess {
                            ty: Type::Error, ..
                        } => true,
                        _ => false,
                    };

                    if is_main && ret_ty.is_result() && is_error_expr {
                        self.emit_main_error_exit()?;
                        return Ok(());
                    }

                    if ret_ty.is_result() && !is_main {
                        if is_error_expr {
                            let result_type = self.llvm_type(&ret_ty).into_struct_type();
                            let flag = self.context.bool_type().const_int(1, false);
                            let result_val = self
                                .builder
                                .build_insert_value(result_type.const_zero(), flag, 1, "ret_error")?
                                .into_struct_value()
                                .as_basic_value_enum();
                            self.builder.build_return(Some(&result_val))?;
                            return Ok(());
                        }

                        if self.hir_expr_type(val) != &ret_ty {
                            llvm_val = self.coerce_type(llvm_val, &ret_ty)?;
                        }
                    } else if !is_error_expr {
                        if is_main {
                            if let BasicValueEnum::IntValue(int_val) = llvm_val {
                                let i64_type = self.context.i64_type();
                                if int_val.get_type() != i64_type {
                                    llvm_val = self
                                        .builder
                                        .build_int_cast(int_val, i64_type, "ret_i64")?
                                        .into();
                                }
                            }
                        } else if self.hir_expr_type(val) != &ret_ty {
                            llvm_val = self.coerce_type(llvm_val, &ret_ty)?;
                        }
                    }

                    self.builder.build_return(Some(&llvm_val))?;
                } else {
                    if let Some(default_ret) = self.default_llvm_return_value(&ret_ty, is_main) {
                        self.builder.build_return(Some(&default_ret))?;
                    } else {
                        self.builder.build_return(None)?;
                    }
                }
                Ok(())
            }
            hir::HirStmt::If {
                condition,
                capture,
                then_branch,
                else_branch,
                ..
            } => self.generate_hir_if_stmt(condition, capture, then_branch, else_branch),
            hir::HirStmt::For {
                label,
                var_name,
                index_var,
                iterable,
                body,
                span: _,
            } => self.generate_hir_for_stmt(label, var_name, index_var, iterable, body),
            hir::HirStmt::Switch { condition, cases, .. } => {
                self.generate_hir_switch_stmt(condition, cases)
            }
            hir::HirStmt::Defer { stmt, .. } => {
                // Add the deferred statement to the current scope's defer list
                // It will be executed in LIFO order when the scope exits
                self.add_defer((**stmt).clone());
                Ok(())
            }
            hir::HirStmt::DeferBang { stmt, .. } => {
                // Add the deferred! statement to the current scope's defer! list
                // It will be executed only when an error occurs (try returns error but not caught)
                self.add_defer_bang((**stmt).clone());
                Ok(())
            }
            hir::HirStmt::Break { label, span } => {
                // For labeled breaks, search through all loop levels
                // For unlabeled breaks, use the innermost loop
                let mut target_block = None;

                // Iterate through all loop levels (from innermost to outermost)
                for loop_stack in self.loop_end_blocks.iter().rev() {
                    // Find the appropriate end block in this loop level
                    if let Some(block) = loop_stack.iter().find_map(|(block, l)| {
                        if l.as_ref() == label.as_ref() {
                            Some(*block)
                        } else if label.is_none() {
                            // If no label specified, use this loop
                            Some(*block)
                        } else {
                            None
                        }
                    }) {
                        target_block = Some(block);
                        break;
                    }
                }

                if let Some(target_block) = target_block {
                    self.builder.build_unconditional_branch(target_block)?;
                } else if label.is_some() {
                    return Err(format!(
                        "break statement with label '{}' not found in scope at span {:?}",
                        label.as_deref().unwrap(),
                        span
                    )
                    .into());
                } else {
                    return Err(
                        format!("break statement outside of loop at span {:?}", span).into(),
                    );
                }
                Ok(())
            }
        }
    }
}
