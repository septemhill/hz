use super::*;
use crate::debug;

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
                            if debug::debug_enabled() {
                                eprintln!("DEBUG: Let - identifier is a function: {}", fn_name);
                            }
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
            hir::HirStmt::Assign {
                target, op, value, ..
            } => {
                if *op != AssignOp::Assign {
                    return Err(
                        format!("Compound assignment {:?} should have been lowered", op).into(),
                    );
                }

                // Skip underscore assignment (used for ignoring values)
                if target == "_" {
                    let _llvm_val = self.generate_hir_expr(value)?;
                    // Just evaluate and discard
                    return Ok(());
                }

                // Improved target parsing to handle both . and []
                // Examples: "a", "a.b", "a[0]", "a.b[0]", "ptr.*"
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
                            // Check for .*
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
                            // Collect everything until ]
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

                let base_name = &parts[0];

                let mut current_ptr = self
                    .variables
                    .get(base_name)
                    .or_else(|| self.const_variables.get(base_name))
                    .ok_or_else(|| format!("Variable not found: {}", base_name))?
                    .clone();

                let mut current_ty = self
                    .variable_types
                    .get(base_name)
                    .ok_or_else(|| format!("Variable type not found: {}", base_name))?
                    .clone();

                let opaque_ptr_type = self.context.ptr_type(inkwell::AddressSpace::default());

                // Iterate through the parts (starting from the second one)
                for i in 1..parts.len() {
                    let part = &parts[i];

                    if part == "*" {
                        // Dereference
                        current_ptr = self
                            .builder
                            .build_load(opaque_ptr_type, current_ptr, "deref_ptr")?
                            .into_pointer_value();

                        current_ty = match current_ty {
                            Type::Pointer(inner) => inner.as_ref().clone(),
                            _ => return Err("Dereference on non-pointer type".into()),
                        };
                    } else if part.starts_with('[') && part.ends_with(']') {
                        // Indexing
                        let index_str = &part[1..part.len() - 1];
                        let index_val = index_str
                            .parse::<u64>()
                            .map_err(|_| "Invalid index in assignment target")?;
                        let index_llvm = self.context.i64_type().const_int(index_val, false);

                        match &current_ty {
                            Type::Array { size, element_type } => {
                                let element_ty = element_type.as_ref().clone();
                                let element_llvm = self.llvm_type(&element_ty);

                                match size {
                                    Some(n) => {
                                        // Array: [N]T
                                        let array_type = element_llvm.array_type(*n as u32);
                                        let zero = self.context.i64_type().const_zero();
                                        current_ptr = unsafe {
                                            self.builder.build_in_bounds_gep(
                                                array_type,
                                                current_ptr,
                                                &[zero, index_llvm],
                                                "index_ptr",
                                            )
                                        }?;
                                    }
                                    None => {
                                        // Slice: []T - load the struct first
                                        let slice_val = self
                                            .builder
                                            .build_load(
                                                self.llvm_type(&current_ty),
                                                current_ptr,
                                                "slice_val",
                                            )?
                                            .into_struct_value();
                                        let ptr_val = self
                                            .builder
                                            .build_extract_value(slice_val, 0, "slice_ptr")?
                                            .into_pointer_value();
                                        current_ptr = unsafe {
                                            self.builder.build_in_bounds_gep(
                                                element_llvm,
                                                ptr_val,
                                                &[index_llvm],
                                                "index_ptr",
                                            )
                                        }?;
                                    }
                                }
                                current_ty = element_ty;
                            }
                            _ => {
                                return Err(
                                    format!("Indexing on non-array type: {:?}", current_ty).into()
                                )
                            }
                        }
                    } else {
                        // Member access
                        let struct_name = match &current_ty {
                            Type::Pointer(inner) => {
                                // If it's a pointer, we need to load it first to get to the struct
                                current_ptr = self
                                    .builder
                                    .build_load(opaque_ptr_type, current_ptr, "struct_ptr")?
                                    .into_pointer_value();

                                let inner_ty = inner.as_ref().clone();
                                if let Type::Custom { name, .. } = inner_ty {
                                    current_ty = Type::Custom {
                                        name: name.clone(),
                                        generic_args: Vec::new(),
                                        is_exported: false,
                                    };
                                    name
                                } else {
                                    return Err("Member access on non-custom pointer type".into());
                                }
                            }
                            Type::Custom { name, .. } => name.clone(),
                            _ => {
                                return Err(format!(
                                    "Member access on non-struct type: {:?}",
                                    current_ty
                                )
                                .into())
                            }
                        };

                        let field_idx = self
                            .struct_field_indices
                            .get(&struct_name)
                            .and_then(|fields| fields.get(part))
                            .copied()
                            .ok_or_else(|| {
                                format!("Field '{}' not found in struct '{}'", part, struct_name)
                            })?;

                        let struct_type = self
                            .context
                            .get_struct_type(&struct_name)
                            .ok_or_else(|| format!("Struct type not found: {}", struct_name))?;

                        current_ptr = self
                            .builder
                            .build_struct_gep(struct_type, current_ptr, field_idx, "field_ptr")
                            .map_err(|e| e.to_string())?;

                        // Update current_ty to the field type
                        let struct_def = self.structs.get(&struct_name).ok_or_else(|| {
                            format!("Struct definition not found: {}", struct_name)
                        })?;
                        let field_def = struct_def
                            .fields
                            .iter()
                            .find(|f| f.name == *part)
                            .ok_or_else(|| {
                                format!("Field '{}' not found in struct '{}'", part, struct_name)
                            })?;
                        current_ty = field_def.ty.clone();
                    }
                }

                // Now we have the final pointer to store into
                let mut llvm_val = self.generate_hir_expr(value)?;
                llvm_val = self.coerce_type(llvm_val, &current_ty)?;

                self.builder.build_store(current_ptr, llvm_val)?;
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
            hir::HirStmt::Switch {
                condition, cases, ..
            } => self.generate_hir_switch_stmt(condition, cases),
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
            hir::HirStmt::Continue { label, span } => {
                // For labeled continues, search through all loop levels
                // For unlabeled continues, use the innermost loop
                let mut target_block = None;

                // Iterate through all loop levels (from innermost to outermost)
                for loop_stack in self.loop_continue_blocks.iter().rev() {
                    // Find the appropriate continue block in this loop level
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
                        "continue statement with label '{}' not found in scope at span {:?}",
                        label.as_deref().unwrap(),
                        span
                    )
                    .into());
                } else {
                    return Err(
                        format!("continue statement outside of loop at span {:?}", span).into(),
                    );
                }
                Ok(())
            }
        }
    }
}
