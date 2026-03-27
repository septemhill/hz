use super::*;

#[allow(unused)]
impl<'ctx> CodeGenerator<'ctx> {
    pub(crate) fn generate_hir_if_stmt(
        &mut self,
        condition: &hir::HirExpr,
        capture: &Option<String>,
        then_branch: &hir::HirStmt,
        else_branch: &Option<Box<hir::HirStmt>>,
    ) -> CodegenResult<()> {
        let cond_val = self.generate_hir_expr(condition)?;
        let function = self.current_function.unwrap();
        let then_block = self.context.append_basic_block(function, "then");
        let else_block = self.context.append_basic_block(function, "else");
        let merge_block = self.context.append_basic_block(function, "cont");

        let is_true = self.condition_to_i1(cond_val, "is_true")?;
        self.builder
            .build_conditional_branch(is_true, then_block, else_block)?;

        self.builder.position_at_end(then_block);

        let mut old_var = None;
        let mut old_ty = None;
        let capture_name = capture.clone();
        if let Some(ref name) = capture_name {
            if let BasicValueEnum::StructValue(sv) = cond_val {
                let val = self.builder.build_extract_value(sv, 0, "captured")?;
                let alloca = self.builder.build_alloca(val.get_type(), name)?;
                self.builder.build_store(alloca, val)?;
                old_var = self.variables.insert(name.clone(), alloca);
                old_ty = self
                    .variable_types
                    .insert(name.clone(), self.llvm_type_to_lang(&val.get_type()));
            }
        }

        self.generate_hir_stmt(then_branch)?;
        if !self.current_block_has_terminator() {
            self.builder.build_unconditional_branch(merge_block)?;
        }

        if let Some(ref name) = capture_name {
            if let Some(old) = old_var {
                self.variables.insert(name.clone(), old);
            } else {
                self.variables.remove(name);
            }

            if let Some(old) = old_ty {
                self.variable_types.insert(name.clone(), old);
            } else {
                self.variable_types.remove(name);
            }
        }

        self.builder.position_at_end(else_block);
        if let Some(eb) = else_branch {
            self.generate_hir_stmt(eb)?;
            if !self.current_block_has_terminator() {
                self.builder.build_unconditional_branch(merge_block)?;
            }
        } else {
            self.builder.build_unconditional_branch(merge_block)?;
        }

        self.builder.position_at_end(merge_block);
        Ok(())
    }

    pub(crate) fn generate_hir_for_stmt(
        &mut self,
        label: &Option<String>,
        var_name: &Option<String>,
        index_var: &Option<String>,
        iterable: &hir::HirExpr,
        body: &hir::HirStmt,
    ) -> CodegenResult<()> {
        let function = self.current_function.unwrap();

        let eval_start_block = self.context.append_basic_block(function, "for_eval");
        let cond_block = self.context.append_basic_block(function, "for_cond");
        let body_block = self.context.append_basic_block(function, "for_body");
        let end_block = self.context.append_basic_block(function, "for_end");

        let iter_val = self.generate_hir_expr(iterable)?;
        let iter_type = iter_val.get_type();

        eprintln!(
            "DEBUG for loop: iter_val type = {:?}, LLVM type = {:?}",
            iterable, iter_type
        );

        let iter_alloca = self.builder.build_alloca(iter_type, "iter_var")?;
        self.builder.build_store(iter_alloca, iter_val)?;

        self.builder.build_unconditional_branch(eval_start_block)?;
        self.builder.position_at_end(eval_start_block);

        let iter_lang_type = match iterable {
            hir::HirExpr::Int(_, ty, _) => Some(ty),
            hir::HirExpr::Float(_, ty, _) => Some(ty),
            hir::HirExpr::Bool(_, ty, _) => Some(ty),
            hir::HirExpr::String(_, ty, _) => Some(ty),
            hir::HirExpr::Char(_, ty, _) => Some(ty),
            hir::HirExpr::Null(ty, _) => Some(ty),
            hir::HirExpr::Ident(_, ty, _) => Some(ty),
            hir::HirExpr::Tuple { ty, .. } => Some(ty),
            hir::HirExpr::TupleIndex { ty, .. } => Some(ty),
            hir::HirExpr::Array { ty, .. } => Some(ty),
            hir::HirExpr::Binary { ty, .. } => Some(ty),
            hir::HirExpr::Unary { ty, .. } => Some(ty),
            hir::HirExpr::Call { return_ty, .. } => Some(return_ty),
            hir::HirExpr::If { ty, .. } => Some(ty),
            hir::HirExpr::Block { ty, .. } => Some(ty),
            hir::HirExpr::MemberAccess { ty, .. } => Some(ty),
            hir::HirExpr::Struct { ty, .. } => Some(ty),
            hir::HirExpr::Try { .. } => None,
            hir::HirExpr::Catch { .. } => None,
            hir::HirExpr::Cast { ty, .. } => Some(ty),
        };

        let mut is_option = false;
        let mut is_bool = false;
        let mut is_array = false;

        if let Some(lang_ty) = iter_lang_type {
            if let Type::Array { .. } = lang_ty {
                is_array = true;
            }
            if let Type::Bool = lang_ty {
                eprintln!(
                    "DEBUG: Setting is_bool = true because lang_ty = {:?}",
                    lang_ty
                );
                is_bool = true;
            }
        }

        if !is_array {
            if iter_type.is_struct_type() {
                let struct_type = iter_type.into_struct_type();
                let types = struct_type.get_field_types();
                if types.len() == 2 {
                    if let Some(t) = types.get(1) {
                        if t.is_int_type() && t.into_int_type().get_bit_width() == 1 {
                            is_option = true;
                        }
                    }
                }
            } else if iter_type.is_int_type() && iter_type.into_int_type().get_bit_width() == 1 {
                eprintln!("DEBUG: Setting is_bool = true because LLVM type is i1");
                is_bool = true;
            } else if iter_type.is_array_type() {
                is_array = true;
            }
        }

        eprintln!(
            "DEBUG: is_option = {}, is_bool = {}, is_array = {}",
            is_option, is_bool, is_array
        );

        let array_index_alloca = if is_array {
            let idx = self
                .builder
                .build_alloca(self.context.i64_type(), "array_index")?;
            self.builder
                .build_store(idx, self.context.i64_type().const_int(0, false))?;
            Some(idx)
        } else {
            None
        };
        self.builder.build_unconditional_branch(cond_block)?;

        self.loop_end_blocks.push(vec![(end_block, label.clone())]);

        self.builder.position_at_end(cond_block);
        let iter_val_load = self
            .builder
            .build_load(iter_type, iter_alloca, "iter_load")?;

        if is_option {
            let is_valid = self
                .builder
                .build_extract_value(iter_val_load.into_struct_value(), 1, "is_valid")?
                .into_int_value();
            let is_null = self.builder.build_int_compare(
                inkwell::IntPredicate::EQ,
                is_valid,
                self.context.bool_type().const_int(0, false),
                "is_null",
            )?;
            self.builder
                .build_conditional_branch(is_null, body_block, end_block)?;
        } else if is_bool {
            eprintln!("DEBUG: is_bool branch, iter_val_load = {:?}", iter_val_load);
            self.builder.build_conditional_branch(
                iter_val_load.into_int_value(),
                body_block,
                end_block,
            )?;
        } else if iter_type.is_struct_type() {
            let struct_type = iter_type.into_struct_type();
            let types = struct_type.get_field_types();
            let mut handled_null_or_option = false;
            if types.len() == 2 {
                if let Some(t) = types.get(1) {
                    if t.is_int_type() && t.into_int_type().get_bit_width() == 1 {
                        let is_valid = self.builder.build_extract_value(
                            iter_val_load.into_struct_value(),
                            1,
                            "is_valid",
                        )?;
                        let is_valid_flag = is_valid.into_int_value();
                        let is_null = self.builder.build_int_compare(
                            inkwell::IntPredicate::EQ,
                            is_valid_flag,
                            self.context.bool_type().const_int(0, false),
                            "is_null",
                        )?;
                        self.builder
                            .build_conditional_branch(is_null, end_block, body_block)?;
                        handled_null_or_option = true;
                    }
                }
            }
            if !handled_null_or_option {
                let start = self.builder.build_extract_value(
                    iter_val_load.into_struct_value(),
                    0,
                    "range_start",
                )?;
                let end = self.builder.build_extract_value(
                    iter_val_load.into_struct_value(),
                    1,
                    "range_end",
                )?;
                let condition_flag = self.builder.build_int_compare(
                    inkwell::IntPredicate::SLT,
                    start.into_int_value(),
                    end.into_int_value(),
                    "range_cmp",
                )?;
                self.builder
                    .build_conditional_branch(condition_flag, body_block, end_block)?;
            }
        } else if is_array || iter_type.is_array_type() {
            if let Some(idx_alloca) = array_index_alloca {
                let current_index = self.builder.build_load(
                    self.context.i64_type(),
                    idx_alloca,
                    "array_idx_load",
                )?;
                let array_type = iter_type.into_array_type();
                let len = array_type.len();
                let len_val = self.context.i64_type().const_int(len as u64, false);
                let condition_flag = self.builder.build_int_compare(
                    inkwell::IntPredicate::SLT,
                    current_index.into_int_value(),
                    len_val,
                    "array_cmp",
                )?;
                self.builder
                    .build_conditional_branch(condition_flag, body_block, end_block)?;
            } else {
                self.builder.build_unconditional_branch(body_block)?;
            }
        } else if iter_type.is_pointer_type() {
            self.builder.build_unconditional_branch(body_block)?;
        } else {
            self.builder.build_unconditional_branch(end_block)?;
        }

        self.builder.position_at_end(body_block);

        let var_name_clone = var_name.clone();
        if let Some(name) = &var_name_clone {
            if is_option || iter_type.is_struct_type() {
                let val = self.builder.build_extract_value(
                    iter_val_load.into_struct_value(),
                    0,
                    "captured",
                )?;
                let alloca = self.builder.build_alloca(val.get_type(), name)?;
                self.builder.build_store(alloca, val)?;
                self.variables.insert(name.clone(), alloca);
                self.variable_types.insert(name.clone(), Type::I64);
            } else if is_array {
                if let Some(idx_alloca) = array_index_alloca {
                    let current_index = self.builder.build_load(
                        self.context.i64_type(),
                        idx_alloca,
                        "array_idx_load_var",
                    )?;
                    let array_type = iter_type.into_array_type();
                    let element_type = array_type.get_element_type();
                    let ptr = unsafe {
                        self.builder.build_in_bounds_gep(
                            iter_type,
                            iter_alloca,
                            &[
                                self.context.i64_type().const_int(0, false),
                                current_index.into_int_value(),
                            ],
                            "array_elem_ptr",
                        )?
                    };
                    let elem_val = self.builder.build_load(element_type, ptr, "array_elem")?;
                    let alloca = self.builder.build_alloca(element_type, name)?;
                    self.builder.build_store(alloca, elem_val)?;
                    self.variables.insert(name.clone(), alloca);
                    let element_lang_type = match element_type {
                        BasicTypeEnum::IntType(t) => match t.get_bit_width() {
                            8 => Type::I8,
                            16 => Type::I16,
                            32 => Type::I32,
                            64 => Type::I64,
                            _ => Type::I64,
                        },
                        BasicTypeEnum::FloatType(_) => Type::F64,
                        _ => Type::I64,
                    };
                    self.variable_types.insert(name.clone(), element_lang_type);
                } else {
                    let alloca = self.builder.build_alloca(self.context.i64_type(), name)?;
                    self.builder
                        .build_store(alloca, self.context.i64_type().const_zero())?;
                    self.variables.insert(name.clone(), alloca);
                    self.variable_types.insert(name.clone(), Type::I64);
                }
            } else {
                let alloca = self.builder.build_alloca(self.context.i64_type(), name)?;
                self.builder
                    .build_store(alloca, self.context.i64_type().const_zero())?;
                self.variables.insert(name.clone(), alloca);
                self.variable_types.insert(name.clone(), Type::I64);
            }
        }

        let index_var_clone = index_var.clone();
        if let Some(name) = &index_var_clone {
            if iter_type.is_struct_type() {
                let val = self.builder.build_extract_value(
                    iter_val_load.into_struct_value(),
                    1,
                    "index_captured",
                )?;
                let alloca = self.builder.build_alloca(val.get_type(), name)?;
                self.builder.build_store(alloca, val)?;
                self.variables.insert(name.clone(), alloca);
                self.variable_types.insert(name.clone(), Type::I64);
            } else if is_array {
                if let Some(idx_alloca) = array_index_alloca {
                    let current_index = self.builder.build_load(
                        self.context.i64_type(),
                        idx_alloca,
                        "array_idx_for_var",
                    )?;
                    let alloca = self.builder.build_alloca(self.context.i64_type(), name)?;
                    self.builder.build_store(alloca, current_index)?;
                    self.variables.insert(name.clone(), alloca);
                    self.variable_types.insert(name.clone(), Type::I64);
                } else {
                    let alloca = self.builder.build_alloca(self.context.i64_type(), name)?;
                    self.builder
                        .build_store(alloca, self.context.i64_type().const_zero())?;
                    self.variables.insert(name.clone(), alloca);
                    self.variable_types.insert(name.clone(), Type::I64);
                }
            } else {
                let alloca = self.builder.build_alloca(self.context.i64_type(), name)?;
                self.builder
                    .build_store(alloca, self.context.i64_type().const_zero())?;
                self.variables.insert(name.clone(), alloca);
                self.variable_types.insert(name.clone(), Type::I64);
            }
        }

        self.generate_hir_stmt(body)?;

        let current_block = self.builder.get_insert_block().unwrap();
        if current_block.get_terminator().is_none() {
            if is_option || is_bool {
                self.builder.build_unconditional_branch(eval_start_block)?;
            } else if iter_type.is_struct_type() {
                let current_load = self
                    .builder
                    .build_load(iter_type, iter_alloca, "iter_next")?;
                let current_struct = current_load.into_struct_value();
                let start = self
                    .builder
                    .build_extract_value(current_struct, 0, "next_start")?;
                let end = self
                    .builder
                    .build_extract_value(current_struct, 1, "next_end")?;
                let incremented_start = self.builder.build_int_add(
                    start.into_int_value(),
                    self.context.i64_type().const_int(1, false),
                    "start_inc",
                )?;

                let mut new_struct: inkwell::values::AggregateValueEnum =
                    iter_type.into_struct_type().const_zero().into();
                new_struct = self
                    .builder
                    .build_insert_value(
                        new_struct.into_struct_value(),
                        incremented_start,
                        0,
                        "new_iter",
                    )?
                    .into();
                new_struct = self
                    .builder
                    .build_insert_value(new_struct.into_struct_value(), end, 1, "new_iter_final")?
                    .into();
                self.builder
                    .build_store(iter_alloca, new_struct.as_basic_value_enum())?;

                self.builder.build_unconditional_branch(cond_block)?;
            } else if is_array {
                if let Some(idx_alloca) = array_index_alloca {
                    let current_index = self.builder.build_load(
                        self.context.i64_type(),
                        idx_alloca,
                        "array_idx_inc",
                    )?;
                    let incremented = self.builder.build_int_add(
                        current_index.into_int_value(),
                        self.context.i64_type().const_int(1, false),
                        "array_idx_next",
                    )?;
                    self.builder.build_store(idx_alloca, incremented)?;
                }
                self.builder.build_unconditional_branch(cond_block)?;
            } else {
                self.builder.build_unconditional_branch(cond_block)?;
            }
        }

        self.builder.position_at_end(end_block);
        self.loop_end_blocks.pop();

        if let Some(name) = &var_name_clone {
            self.variables.remove(name);
            self.variable_types.remove(name);
        }
        if let Some(name) = &index_var_clone {
            self.variables.remove(name);
            self.variable_types.remove(name);
        }

        Ok(())
    }

    pub(crate) fn generate_hir_switch_stmt(
        &mut self,
        condition: &hir::HirExpr,
        cases: &[hir::HirCase],
    ) -> CodegenResult<()> {
        let function = self.current_function.unwrap();
        let end_block = self.context.append_basic_block(function, "switch_end");
        let cond_val = self.generate_hir_expr(condition)?;

        for case in cases {
            let body_block = self.context.append_basic_block(function, "case_body");
            let next_case_block = self.context.append_basic_block(function, "next_case");

            let mut is_wildcard = false;
            for pattern in &case.patterns {
                if let hir::HirExpr::Ident(name, ..) = pattern {
                    if name == "_" {
                        is_wildcard = true;
                        break;
                    }
                }
            }

            if is_wildcard {
                self.builder.build_unconditional_branch(body_block)?;
            } else {
                let mut combined_cond: Option<inkwell::values::IntValue> = None;
                for pattern in &case.patterns {
                    let pattern_val = self.generate_hir_expr(pattern)?;

                    let cmp = if let hir::HirExpr::Binary {
                        op: crate::ast::BinaryOp::Range,
                        left,
                        right,
                        ..
                    } = pattern
                    {
                        let start = self.generate_hir_expr(left)?;
                        let end = self.generate_hir_expr(right)?;
                        let cond_int = cond_val.into_int_value();
                        let cond_type = cond_int.get_type();

                        // Cast start and end to match the condition value's type
                        let start_int = self.builder.build_int_cast(
                            start.into_int_value(),
                            cond_type,
                            "range_start_cast",
                        )?;
                        let end_int = self.builder.build_int_cast(
                            end.into_int_value(),
                            cond_type,
                            "range_end_cast",
                        )?;

                        let ge = self.builder.build_int_compare(
                            inkwell::IntPredicate::UGE,
                            cond_int,
                            start_int,
                            "range_ge",
                        )?;
                        let lt = self.builder.build_int_compare(
                            inkwell::IntPredicate::ULT,
                            cond_int,
                            end_int,
                            "range_lt",
                        )?;
                        self.builder.build_and(ge, lt, "range_check")?
                    } else {
                        self.builder.build_int_compare(
                            inkwell::IntPredicate::EQ,
                            cond_val.into_int_value(),
                            pattern_val.into_int_value(),
                            "case_cmp",
                        )?
                    };
                    combined_cond = if let Some(c) = combined_cond {
                        Some(self.builder.build_or(c, cmp, "or")?)
                    } else {
                        Some(cmp)
                    };
                }

                if let Some(cond) = combined_cond {
                    self.builder
                        .build_conditional_branch(cond, body_block, next_case_block)?;
                } else {
                    self.builder.build_unconditional_branch(next_case_block)?;
                }
            }

            self.builder.position_at_end(body_block);
            self.generate_hir_stmt(&case.body)?;
            if self
                .builder
                .get_insert_block()
                .unwrap()
                .get_terminator()
                .is_none()
            {
                self.builder.build_unconditional_branch(end_block)?;
            }

            self.builder.position_at_end(next_case_block);
        }

        if self
            .builder
            .get_insert_block()
            .unwrap()
            .get_terminator()
            .is_none()
        {
            self.builder.build_unconditional_branch(end_block)?;
        }

        self.builder.position_at_end(end_block);
        Ok(())
    }

    pub(crate) fn generate_hir_if_expr(
        &mut self,
        condition: &hir::HirExpr,
        capture: &Option<String>,
        then_branch: &hir::HirExpr,
        else_branch: &hir::HirExpr,
        ty: &Type,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let cond_val = self.generate_hir_expr(condition)?;
        let function = self.current_function.unwrap();
        let then_block = self.context.append_basic_block(function, "then");
        let else_block = self.context.append_basic_block(function, "else");
        let merge_block = self.context.append_basic_block(function, "cont");

        let is_true = self.condition_to_i1(cond_val, "is_true")?;
        self.builder
            .build_conditional_branch(is_true, then_block, else_block)?;

        self.builder.position_at_end(then_block);

        let mut old_var = None;
        let mut old_ty = None;
        let capture_name = capture.clone();
        if let Some(ref name) = capture_name {
            if let BasicValueEnum::StructValue(sv) = cond_val {
                let val = self.builder.build_extract_value(sv, 0, "captured")?;
                let alloca = self.builder.build_alloca(val.get_type(), name)?;
                self.builder.build_store(alloca, val)?;
                old_var = self.variables.insert(name.clone(), alloca);
                old_ty = self
                    .variable_types
                    .insert(name.clone(), self.llvm_type_to_lang(&val.get_type()));
            }
        }

        let result_type = self.llvm_type(ty);

        let then_val = self.generate_hir_expr(then_branch)?;
        let then_val_coerced = self.coerce_to_llvm_type(then_val, result_type)?;
        let then_actual_block = self.builder.get_insert_block().unwrap();
        self.builder.build_unconditional_branch(merge_block)?;

        if let Some(ref name) = capture_name {
            if let Some(old) = old_var {
                self.variables.insert(name.clone(), old);
            } else {
                self.variables.remove(name);
            }

            if let Some(old) = old_ty {
                self.variable_types.insert(name.clone(), old);
            } else {
                self.variable_types.remove(name);
            }
        }

        self.builder.position_at_end(else_block);
        let else_val = self.generate_hir_expr(else_branch)?;
        let else_val_coerced = self.coerce_to_llvm_type(else_val, result_type)?;
        let else_actual_block = self.builder.get_insert_block().unwrap();
        self.builder.build_unconditional_branch(merge_block)?;

        self.builder.position_at_end(merge_block);

        let phi = self.builder.build_phi(result_type, "if_result")?;
        phi.add_incoming(&[
            (&then_val_coerced, then_actual_block),
            (&else_val_coerced, else_actual_block),
        ]);

        Ok(phi.as_basic_value())
    }

    pub(crate) fn generate_hir_block_expr(
        &mut self,
        stmts: &[hir::HirStmt],
        expr: &Option<Box<hir::HirExpr>>,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        for stmt in stmts {
            self.generate_hir_stmt(stmt)?;
        }
        if let Some(e) = expr {
            self.generate_hir_expr(e)
        } else {
            Ok(self.context.i64_type().const_int(0, false).into())
        }
    }

    pub(crate) fn generate_hir_try_expr(
        &mut self,
        expr: &hir::HirExpr,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let expr_value = self.generate_hir_expr(expr)?;

        let is_result = self
            .return_type
            .as_ref()
            .map(|t| t.is_result())
            .unwrap_or(false);

        if is_result {
            let result_struct = match expr_value {
                BasicValueEnum::StructValue(sv) => sv,
                _ => return Err("Try expression requires a result value".into()),
            };
            let is_error = self
                .builder
                .build_extract_value(result_struct, 1, "is_error")?
                .into_int_value();

            let function = self.current_function.unwrap();
            let error_block = self.context.append_basic_block(function, "try_error");
            let continue_block = self.context.append_basic_block(function, "try_continue");

            self.builder
                .build_conditional_branch(is_error, error_block, continue_block)?;

            self.builder.position_at_end(error_block);
            self.execute_defer_bang_on_error()?;
            if self.current_function_is_main() {
                self.emit_main_error_exit()?;
            } else {
                let result_value = result_struct.as_basic_value_enum();
                self.builder.build_return(Some(&result_value))?;
            }

            self.builder.position_at_end(continue_block);
            let value = self
                .builder
                .build_extract_value(result_struct, 0, "try_value")?;
            Ok(value.into())
        } else {
            Ok(expr_value)
        }
    }

    pub(crate) fn generate_hir_catch_expr(
        &mut self,
        expr: &hir::HirExpr,
        body: &hir::HirExpr,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let expr_value = self.generate_hir_expr(expr)?;

        let result_struct = match expr_value {
            BasicValueEnum::StructValue(sv) => sv,
            _ => return Err("Catch expression requires a result value".into()),
        };

        let is_error = self
            .builder
            .build_extract_value(result_struct, 1, "is_error")?
            .into_int_value();
        let inner_value = self
            .builder
            .build_extract_value(result_struct, 0, "inner_value")?;

        let branch_block = self.builder.get_insert_block().unwrap();

        let function = self.current_function.unwrap();
        let catch_block = self.context.append_basic_block(function, "catch_body");
        let continue_block = self.context.append_basic_block(function, "catch_continue");

        self.builder
            .build_conditional_branch(is_error, catch_block, continue_block)?;

        self.builder.position_at_end(catch_block);
        let body_value = self.generate_hir_expr(body)?;
        self.builder.build_unconditional_branch(continue_block)?;

        self.builder.position_at_end(continue_block);

        let phi = self
            .builder
            .build_phi(inner_value.get_type(), "catch_result")?;
        phi.add_incoming(&[(&body_value, catch_block), (&inner_value, branch_block)]);
        Ok(phi.as_basic_value())
    }
}
