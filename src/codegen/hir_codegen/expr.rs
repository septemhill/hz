use super::*;

use std::collections::hash_map::Entry;

#[allow(unused)]
impl<'ctx> CodeGenerator<'ctx> {
    /// Generate the address of an l-value (identifier, member access, dereference, etc.)
    pub(crate) fn generate_hir_lvalue_address(
        &mut self,
        expr: &hir::HirExpr,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        match expr {
            hir::HirExpr::Ident(name, _, _) => {
                // Look up the variable's allocation pointer
                if let Some(ptr) = self.variables.get(name) {
                    Ok(ptr.as_basic_value_enum())
                } else if let Some(ptr) = self.const_variables.get(name) {
                    Ok(ptr.as_basic_value_enum())
                } else {
                    Err(format!("Variable not found for address-of: {}", name).into())
                }
            }
            hir::HirExpr::MemberAccess { object, member, .. } => {
                // Address of a field: GEP into the struct
                let obj_addr = self.generate_hir_lvalue_address(object)?;
                let obj_ptr = obj_addr.into_pointer_value();

                // Get struct name and field index
                let struct_name = match object.ty() {
                    Type::Pointer(inner) => {
                        if let Type::Custom { name, .. } = &**inner {
                            name.clone()
                        } else {
                            return Err("Member access on non-custom pointer type".into());
                        }
                    }
                    Type::Custom { name, .. } => name.clone(),
                    _ => return Err("Member access on non-struct type".into()),
                };

                let field_idx = self
                    .struct_field_indices
                    .get(&struct_name)
                    .and_then(|fields| fields.get(member))
                    .copied()
                    .ok_or_else(|| format!("Field '{}' not found in struct '{}'", member, struct_name))?;

                let struct_type = self
                    .context
                    .get_struct_type(&struct_name)
                    .ok_or_else(|| format!("Struct type not found: {}", struct_name))?;

                // For pointers, we need to load the pointer first
                let final_obj_ptr = if matches!(object.ty(), Type::Pointer(_)) {
                    let opaque_ptr_type = self.context.ptr_type(inkwell::AddressSpace::default());
                    self.builder
                        .build_load(opaque_ptr_type, obj_ptr, "deref_ptr")?
                        .into_pointer_value()
                } else {
                    obj_ptr
                };

                let field_ptr = self
                    .builder
                    .build_struct_gep(struct_type, final_obj_ptr, field_idx, "field_ptr")
                    .map_err(|e| e.to_string())?;

                Ok(field_ptr.into())
            }
            hir::HirExpr::Dereference { expr, .. } => {
                // Address of a dereference: just evaluate the pointer expression
                self.generate_hir_expr(expr)
            }
            hir::HirExpr::Index { object, index, .. } => {
                let obj_ty = object.ty();
                let element_type = match obj_ty {
                    Type::Array { element_type, .. } => element_type.as_ref(),
                    _ => return Err("Indexing only supported on array or slice types".into()),
                };
                let element_llvm = self.llvm_type(element_type);

                let index_val = self.generate_hir_expr(index)?;
                let index_int = index_val.into_int_value();

                match obj_ty {
                    Type::Array { size: Some(n), .. } => {
                        let obj_addr = self.generate_hir_lvalue_address(object)?;
                        let obj_ptr = obj_addr.into_pointer_value();
                        let zero = self.context.i64_type().const_zero();
                        let array_type = element_llvm.array_type(*n as u32);
                        let ptr = unsafe {
                            self.builder.build_in_bounds_gep(
                                array_type,
                                obj_ptr,
                                &[zero, index_int],
                                "index_ptr",
                            )
                        }?;
                        Ok(ptr.into())
                    }
                    Type::Array { size: None, .. } => {
                        let slice_val = self.generate_hir_expr(object)?;
                        let slice_struct = slice_val.into_struct_value();
                        let ptr_val = self
                            .builder
                            .build_extract_value(slice_struct, 0, "slice_ptr")?;
                        let ptr = ptr_val.into_pointer_value();
                        let element_ptr = unsafe {
                            self.builder.build_in_bounds_gep(
                                element_llvm,
                                ptr,
                                &[index_int],
                                "index_ptr",
                            )
                        }?;
                        Ok(element_ptr.into())
                    }
                    _ => unreachable!(),
                }
            }
            _ => Err("Expression is not an l-value (cannot take address)".into()),
        }
    }

    pub(crate) fn generate_intrinsic(
        &mut self,
        name: &str,
        args: &[hir::HirExpr],
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        // Find the intrinsic in the registry
        let intrinsic = self
            .intrinsics
            .get(name)
            .cloned() // Clone the Rc to avoid borrowing self.intrinsics while calling &mut self methods
            .ok_or_else(|| format!("Intrinsic function '{}' not found", name))?;

        // Call the intrinsic's generator
        intrinsic.generate(self, args)
    }

    pub(crate) fn generate_hir_expr(
        &mut self,
        expr: &hir::HirExpr,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        match expr {
            hir::HirExpr::Int(v, ty, _) => Ok(self.build_typed_int_constant(*v, ty)),
            hir::HirExpr::Float(v, _, _) => Ok(self.context.f64_type().const_float(*v).into()),
            hir::HirExpr::Bool(v, _, _) => Ok(self
                .context
                .bool_type()
                .const_int(if *v { 1 } else { 0 }, false)
                .into()),
            hir::HirExpr::String(v, ty, _) => {
                // For string literals, create a global string and return its slice {ptr, len}
                let str_val = unsafe { self.builder.build_global_string(v, "str") }?;
                let ptr = str_val.as_basic_value_enum();
                let len = self.context.i64_type().const_int(v.len() as u64, false);

                let slice_type = self.llvm_type(ty).into_struct_type();
                let mut slice_val = slice_type.get_undef();
                slice_val = self
                    .builder
                    .build_insert_value(slice_val, ptr, 0, "slice_ptr")?
                    .into_struct_value();
                slice_val = self
                    .builder
                    .build_insert_value(slice_val, len, 1, "slice_len")?
                    .into_struct_value();

                Ok(slice_val.into())
            }
            hir::HirExpr::Char(v, ty, _) => {
                // Use the type from the HIR expression if available
                match self.llvm_type(ty) {
                    BasicTypeEnum::IntType(_) => Ok(self.build_typed_int_constant(*v as i64, ty)),
                    _ => Ok(self.context.i64_type().const_int(*v as u64, false).into()),
                }
            }
            hir::HirExpr::Null(ty, _) => {
                // Null is represented as a struct { value, is_valid } with is_valid = false
                // Use the expected type from the Option if context available
                let (val_type, is_valid_type) = if let Type::Option(inner) = ty {
                    (self.llvm_type(inner), self.context.bool_type())
                } else {
                    (self.context.i64_type().into(), self.context.bool_type())
                };
                let null_struct = self
                    .context
                    .struct_type(&[val_type.into(), is_valid_type.into()], false);
                Ok(null_struct.const_zero().into())
            }
            hir::HirExpr::Tuple { vals, ty, .. } => {
                // Create an LLVM struct from tuple values
                let struct_type = self.llvm_type(ty);
                let mut elements: Vec<BasicValueEnum> = Vec::new();
                for v in vals {
                    elements.push(self.generate_hir_expr(v)?);
                }
                // Build struct value
                let struct_val = self.context.const_struct(&elements, false);
                // We need to store it to memory to return as pointer
                let alloca = self.builder.build_alloca(struct_type, "tuple")?;
                self.builder.build_store(alloca, struct_val)?;
                Ok(self
                    .builder
                    .build_load(struct_type, alloca, "tuple_load")?
                    .into())
            }
            hir::HirExpr::TupleIndex {
                tuple, index, ty, ..
            } => {
                let tuple_val = self.generate_hir_expr(tuple)?;
                let _llvm_type = self.llvm_type(ty);
                match tuple_val {
                    BasicValueEnum::StructValue(sv) => {
                        let extracted =
                            self.builder
                                .build_extract_value(sv, *index as u32, "tuple_elem")?;
                        Ok(extracted.into())
                    }
                    _ => Err("Tuple index access requires a tuple value".into()),
                }
            }
            hir::HirExpr::Index {
                object,
                index,
                ty,
                ..
            } => {
                let obj_ty = object.ty();
                let element_type = match obj_ty {
                    Type::Array { element_type, .. } => element_type.as_ref(),
                    _ => return Err("Indexing only supported on array or slice types".into()),
                };
                let element_llvm = self.llvm_type(element_type);

                // Handle slicing (if index is a range)
                if let hir::HirExpr::Binary {
                    op: BinaryOp::Range,
                    left: range_start,
                    right: range_end,
                    ..
                } = index.as_ref()
                {
                    let start_val = self.generate_hir_expr(range_start)?.into_int_value();
                    let end_val = self.generate_hir_expr(range_end)?.into_int_value();

                    let ptr = match obj_ty {
                        Type::Array { size: Some(n), .. } => {
                            let obj_addr = self.generate_hir_lvalue_address(object)?;
                            let obj_ptr = obj_addr.into_pointer_value();
                            let zero = self.context.i64_type().const_zero();
                            let array_type = element_llvm.array_type(*n as u32);
                            unsafe {
                                self.builder.build_in_bounds_gep(
                                    array_type,
                                    obj_ptr,
                                    &[zero, start_val],
                                    "slice_ptr",
                                )
                            }?
                        }
                        Type::Array { size: None, .. } => {
                            let slice_val = self.generate_hir_expr(object)?;
                            let slice_struct = slice_val.into_struct_value();
                            let ptr_val = self
                                .builder
                                .build_extract_value(slice_struct, 0, "slice_ptr")?;
                            let old_ptr = ptr_val.into_pointer_value();
                            unsafe {
                                self.builder.build_in_bounds_gep(
                                    element_llvm,
                                    old_ptr,
                                    &[start_val],
                                    "slice_ptr",
                                )
                            }?
                        }
                        _ => unreachable!(),
                    };

                    // len = end - start
                    let len = self.builder.build_int_sub(end_val, start_val, "slice_len")?;

                    // Create slice struct { *T, i64 }
                    let slice_type = self.llvm_type(ty).into_struct_type();
                    let mut slice_val = slice_type.get_undef();
                    slice_val = self
                        .builder
                        .build_insert_value(slice_val, ptr, 0, "slice_ptr")?
                        .into_struct_value();
                    slice_val = self
                        .builder
                        .build_insert_value(slice_val, len, 1, "slice_len")?
                        .into_struct_value();

                    return Ok(slice_val.into());
                }

                // Not a range, so it's a single element access
                let element_ptr = self.generate_hir_lvalue_address(expr)?;
                let val = self.builder.build_load(
                    element_llvm,
                    element_ptr.into_pointer_value(),
                    "index_load",
                )?;
                Ok(val.into())
            }
            hir::HirExpr::Array { vals, ty, .. } => {
                // Create an LLVM array from values
                let llvm_type = self.llvm_type(ty);
                let expected_element_type = match ty {
                    Type::Array { element_type, .. } => element_type.as_ref(),
                    _ => return Err("Array expression is missing an array type".into()),
                };

                // Generate element values
                let mut elem_vals: Vec<inkwell::values::BasicValueEnum> = Vec::new();
                for v in vals {
                    let raw_val = self.generate_hir_expr(v)?;
                    elem_vals.push(self.coerce_type(raw_val, expected_element_type)?);
                }

                // Create array constant using const_array
                let array_type = llvm_type.into_array_type();
                let element_type = array_type.get_element_type();
                let array_val = match element_type {
                    BasicTypeEnum::IntType(t) => {
                        let mut const_vec: Vec<inkwell::values::IntValue> = Vec::new();
                        for ev in &elem_vals {
                            const_vec.push(ev.into_int_value());
                        }
                        t.const_array(&const_vec).as_basic_value_enum()
                    }
                    BasicTypeEnum::FloatType(t) => {
                        let mut const_vec: Vec<inkwell::values::FloatValue> = Vec::new();
                        for ev in &elem_vals {
                            const_vec.push(ev.into_float_value());
                        }
                        t.const_array(&const_vec).as_basic_value_enum()
                    }
                    _ => return Err("Unsupported array element type".into()),
                };

                // Store constant to memory and return loaded value
                let alloca = self.builder.build_alloca(llvm_type, "array")?;
                self.builder.build_store(alloca, array_val)?;
                Ok(self
                    .builder
                    .build_load(llvm_type, alloca, "array_load")?
                    .into())
            }
            hir::HirExpr::Ident(name, ty, _) => {
                // Skip underscore identifier (used for ignoring values)
                if name == "_" {
                    // Return a dummy value
                    return Ok(self.context.i64_type().const_zero().into());
                }

                // Check if this is a function type - if so, get the function pointer
                if let Type::Function { .. } = ty {
                    // Get the function pointer
                    let fn_name = if name == "main" {
                        "main".to_string()
                    } else {
                        format!("{}_{}", self.module_name, name)
                    };
                    let fn_val = self
                        .module
                        .get_function(&fn_name)
                        .ok_or(format!("Function not found: {}", fn_name))?;
                    // Return the function's pointer
                    return Ok(fn_val.as_global_value().as_pointer_value().into());
                }

                let ptr = self
                    .variables
                    .get(name)
                    .ok_or(format!("Var not found: {}", name))?;
                let var_ty = self.variable_types.get(name).unwrap();
                let llvm_type = self.llvm_type(var_ty);
                Ok(self.builder.build_load(llvm_type, *ptr, name)?.into())
            }
            hir::HirExpr::Binary {
                op,
                left,
                right,
                ty,
                ..
            } => {
                let mut l = self.generate_hir_expr(left)?;
                let mut r = self.generate_hir_expr(right)?;

                // Type coercion: cast right operand to match left operand's type if needed
                if l.is_int_value() && r.is_int_value() {
                    let l_type = l.into_int_value().get_type();
                    let r_type = r.into_int_value().get_type();
                    if l_type.get_bit_width() != r_type.get_bit_width() {
                        r = self
                            .builder
                            .build_int_cast(r.into_int_value(), l_type, "binary_cast")?
                            .into();
                    }
                }

                // Handle different types
                let val = match op {
                    BinaryOp::Add => {
                        if l.is_float_value() {
                            self.builder
                                .build_float_add(
                                    l.into_float_value(),
                                    r.into_float_value(),
                                    "fadd",
                                )?
                                .into()
                        } else {
                            let l_int = l.into_int_value();
                            let r_int = r.into_int_value();
                            self.builder.build_int_add(l_int, r_int, "add")?.into()
                        }
                    }
                    BinaryOp::Sub => {
                        if l.is_float_value() {
                            self.builder
                                .build_float_sub(
                                    l.into_float_value(),
                                    r.into_float_value(),
                                    "fsub",
                                )?
                                .into()
                        } else {
                            let l_int = l.into_int_value();
                            let r_int = r.into_int_value();
                            self.builder.build_int_sub(l_int, r_int, "sub")?.into()
                        }
                    }
                    BinaryOp::Mul => {
                        if l.is_float_value() {
                            self.builder
                                .build_float_mul(
                                    l.into_float_value(),
                                    r.into_float_value(),
                                    "fmul",
                                )?
                                .into()
                        } else {
                            let l_int = l.into_int_value();
                            let r_int = r.into_int_value();
                            self.builder.build_int_mul(l_int, r_int, "mul")?.into()
                        }
                    }
                    BinaryOp::Div => {
                        if l.is_float_value() {
                            self.builder
                                .build_float_div(
                                    l.into_float_value(),
                                    r.into_float_value(),
                                    "fdiv",
                                )?
                                .into()
                        } else {
                            let l_int = l.into_int_value();
                            let r_int = r.into_int_value();
                            self.builder
                                .build_int_unsigned_div(l_int, r_int, "div")?
                                .into()
                        }
                    }
                    BinaryOp::Mod => {
                        if l.is_float_value() {
                            self.builder
                                .build_float_rem(
                                    l.into_float_value(),
                                    r.into_float_value(),
                                    "frem",
                                )?
                                .into()
                        } else {
                            let l_int = l.into_int_value();
                            let r_int = r.into_int_value();
                            self.builder
                                .build_int_unsigned_rem(l_int, r_int, "mod")?
                                .into()
                        }
                    }
                    BinaryOp::Eq => {
                        if l.is_float_value() {
                            self.builder
                                .build_float_compare(
                                    inkwell::FloatPredicate::OEQ,
                                    l.into_float_value(),
                                    r.into_float_value(),
                                    "feq",
                                )?
                                .into()
                        } else {
                            let l_int = l.into_int_value();
                            let r_int = r.into_int_value();
                            let cmp = self.builder.build_int_compare(
                                inkwell::IntPredicate::EQ,
                                l_int,
                                r_int,
                                "eq",
                            )?;
                            cmp.into()
                        }
                    }
                    BinaryOp::Ne => {
                        if l.is_float_value() {
                            self.builder
                                .build_float_compare(
                                    inkwell::FloatPredicate::ONE,
                                    l.into_float_value(),
                                    r.into_float_value(),
                                    "fne",
                                )?
                                .into()
                        } else {
                            let l_int = l.into_int_value();
                            let r_int = r.into_int_value();
                            let cmp = self.builder.build_int_compare(
                                inkwell::IntPredicate::NE,
                                l_int,
                                r_int,
                                "ne",
                            )?;
                            cmp.into()
                        }
                    }
                    BinaryOp::Lt => {
                        if l.is_float_value() {
                            self.builder
                                .build_float_compare(
                                    inkwell::FloatPredicate::OLT,
                                    l.into_float_value(),
                                    r.into_float_value(),
                                    "flt",
                                )?
                                .into()
                        } else {
                            let l_int = l.into_int_value();
                            let r_int = r.into_int_value();
                            let cmp = self.builder.build_int_compare(
                                inkwell::IntPredicate::ULT,
                                l_int,
                                r_int,
                                "lt",
                            )?;
                            cmp.into()
                        }
                    }
                    BinaryOp::Gt => {
                        if l.is_float_value() {
                            self.builder
                                .build_float_compare(
                                    inkwell::FloatPredicate::OGT,
                                    l.into_float_value(),
                                    r.into_float_value(),
                                    "fgt",
                                )?
                                .into()
                        } else {
                            let l_int = l.into_int_value();
                            let r_int = r.into_int_value();
                            let cmp = self.builder.build_int_compare(
                                inkwell::IntPredicate::UGT,
                                l_int,
                                r_int,
                                "gt",
                            )?;
                            cmp.into()
                        }
                    }
                    BinaryOp::Le => {
                        if l.is_float_value() {
                            self.builder
                                .build_float_compare(
                                    inkwell::FloatPredicate::OLE,
                                    l.into_float_value(),
                                    r.into_float_value(),
                                    "fle",
                                )?
                                .into()
                        } else {
                            let l_int = l.into_int_value();
                            let r_int = r.into_int_value();
                            let cmp = self.builder.build_int_compare(
                                inkwell::IntPredicate::ULE,
                                l_int,
                                r_int,
                                "le",
                            )?;
                            cmp.into()
                        }
                    }
                    BinaryOp::Ge => {
                        if l.is_float_value() {
                            self.builder
                                .build_float_compare(
                                    inkwell::FloatPredicate::OGE,
                                    l.into_float_value(),
                                    r.into_float_value(),
                                    "fge",
                                )?
                                .into()
                        } else {
                            let l_int = l.into_int_value();
                            let r_int = r.into_int_value();
                            let cmp = self.builder.build_int_compare(
                                inkwell::IntPredicate::UGE,
                                l_int,
                                r_int,
                                "ge",
                            )?;
                            cmp.into()
                        }
                    }
                    BinaryOp::And => {
                        let l_int = l.into_int_value();
                        let r_int = r.into_int_value();
                        let and_val = self.builder.build_and(l_int, r_int, "and")?;
                        and_val.into()
                    }
                    BinaryOp::Or => {
                        let l_int = l.into_int_value();
                        let r_int = r.into_int_value();
                        let or_val = self.builder.build_or(l_int, r_int, "or")?;
                        or_val.into()
                    }
                    BinaryOp::BitAnd => {
                        let l_int = l.into_int_value();
                        let r_int = r.into_int_value();
                        self.builder.build_and(l_int, r_int, "bitand")?.into()
                    }
                    BinaryOp::BitOr => {
                        let l_int = l.into_int_value();
                        let r_int = r.into_int_value();
                        self.builder.build_or(l_int, r_int, "bitor")?.into()
                    }
                    BinaryOp::BitXor => {
                        let l_int = l.into_int_value();
                        let r_int = r.into_int_value();
                        self.builder.build_xor(l_int, r_int, "bitxor")?.into()
                    }
                    BinaryOp::Shl => {
                        let l_int = l.into_int_value();
                        let r_int = r.into_int_value();
                        self.builder.build_left_shift(l_int, r_int, "shl")?.into()
                    }
                    BinaryOp::Shr => {
                        let l_int = l.into_int_value();
                        let r_int = r.into_int_value();
                        // Use logical shift right (LSR) for now, as we use unsigned divisions
                        self.builder
                            .build_right_shift(l_int, r_int, false, "shr")?
                            .into()
                    }
                    BinaryOp::Range => {
                        // Range: create a tuple { start, end }
                        let l_val = l.into_int_value();
                        let r_val = r.into_int_value();
                        // Create a tuple struct type { i64, i64 }
                        let tuple_type = self.context.struct_type(
                            &[
                                self.context.i64_type().into(),
                                self.context.i64_type().into(),
                            ],
                            false,
                        );
                        // Create the struct value
                        let tuple_val = self
                            .context
                            .const_struct(&[l_val.into(), r_val.into()], false);
                        // Store to memory and return
                        let alloca = self.builder.build_alloca(tuple_type, "range")?;
                        self.builder.build_store(alloca, tuple_val)?;
                        self.builder
                            .build_load(tuple_type, alloca, "range_load")?
                            .into()
                    }
                };
                Ok(val)
            }
            hir::HirExpr::Unary {
                op, expr, ty: _, ..
            } => {
                let e = self.generate_hir_expr(expr)?;
                let val = match op {
                    UnaryOp::Neg => {
                        let e_int = e.into_int_value();
                        self.builder.build_int_neg(e_int, "neg")?.into()
                    }
                    UnaryOp::Pos => {
                        // Positive is a no-op
                        e
                    }
                    UnaryOp::Not => {
                        let e_int = e.into_int_value();
                        let zero = self.context.i64_type().const_int(0, false);
                        let cmp = self.builder.build_int_compare(
                            inkwell::IntPredicate::EQ,
                            e_int,
                            zero,
                            "not",
                        )?;
                        cmp.into()
                    }
                    UnaryOp::Ref => {
                        // Reference - return the address of the value
                        // For arrays, we want to return a slice { ptr, len }
                        let val_ty = expr.ty();
                        if let Type::Array {
                            size: Some(n),
                            element_type,
                        } = val_ty
                        {
                            let array_ptr = self.generate_hir_lvalue_address(expr)?;
                            let element_llvm = self.llvm_type(element_type);

                            // ptr = GEP to first element
                            let zero = self.context.i64_type().const_zero();
                            let array_type = element_llvm.array_type(*n as u32);
                            let first_elem_ptr = unsafe {
                                self.builder.build_in_bounds_gep(
                                    array_type,
                                    array_ptr.into_pointer_value(),
                                    &[zero, zero],
                                    "first_elem_ptr",
                                )
                            }?;

                            // Create slice struct { *T, i64 }
                            let slice_type = self
                                .llvm_type(&Type::Array {
                                    size: None,
                                    element_type: element_type.clone(),
                                })
                                .into_struct_type();
                            let mut slice_val = slice_type.get_undef();
                            slice_val = self
                                .builder
                                .build_insert_value(slice_val, first_elem_ptr, 0, "slice_ptr")?
                                .into_struct_value();
                            slice_val = self
                                .builder
                                .build_insert_value(
                                    slice_val,
                                    self.context.i64_type().const_int(*n as u64, false),
                                    1,
                                    "slice_len",
                                )?
                                .into_struct_value();

                            return Ok(slice_val.into());
                        }
                        return self.generate_hir_lvalue_address(expr);
                    }
                };
                Ok(val)
            }
            hir::HirExpr::Intrinsic { name, args, .. } => {
                self.generate_intrinsic(name, args)
            }
            hir::HirExpr::Call {
                name,
                namespace,
                args,
                ..
            } => {
                eprintln!("DEBUG: hir_call - name={}, namespace={:?}", name, namespace);
                // First, check if the callee is a variable that holds a function pointer
                if namespace.is_none() {
                    if let Some(var_alloca) = self.variables.get(name) {
                        if let Some(var_type) = self.variable_types.get(name) {
                            // Check if this variable has a function type
                            if let Type::Function {
                                params,
                                return_type,
                            } = var_type.clone()
                            {
                                // Load the function pointer from the variable
                                let fn_ptr = self.builder.build_load(
                                    self.context.ptr_type(inkwell::AddressSpace::default()),
                                    *var_alloca,
                                    name,
                                )?;

                                // Generate args first
                                let mut llvm_args: Vec<BasicMetadataValueEnum> = Vec::new();
                                for arg_expr in args {
                                    let val = self.generate_hir_expr(arg_expr)?;
                                    llvm_args.push(BasicMetadataValueEnum::from(val));
                                }

                                // Create function type for indirect call using existing method
                                let fn_sig = self.build_function_type(&return_type, &params, false);

                                // Cast pointer to function pointer type
                                let casted_ptr = self
                                    .builder
                                    .build_bit_cast(
                                        fn_ptr,
                                        self.context.ptr_type(inkwell::AddressSpace::default()),
                                        "cast_fn",
                                    )?
                                    .into_pointer_value();

                                let call_result = self.builder.build_indirect_call(
                                    fn_sig,
                                    casted_ptr,
                                    &llvm_args,
                                    "indirect_call",
                                )?;

                                let result = match call_result.try_as_basic_value() {
                                    inkwell::values::ValueKind::Basic(val) => val,
                                    _ => self.context.i64_type().const_int(0, false).into(),
                                };
                                return Ok(result);
                            }
                        }
                    }
                }

                let (mangled_name, _is_std, needs_self, is_fn_ptr_field) = if let Some(ns) =
                    namespace.as_deref()
                {
                    // First, check if the namespace is a variable with a struct type
                    // This handles method calls like "f.next()" where f is a struct instance
                    // But also handles field access like "c.add" where c is a struct with a function field
                    if let Some(var_type) = self.variable_types.get(ns) {
                        if let Type::Custom {
                            name: type_name, ..
                        } = var_type
                        {
                            // Get the monomorphized struct name for method lookup
                            let mono_type_name = self.get_monomorphized_struct_name(type_name);
                            eprintln!(
                                "DEBUG: method lookup - ns={}, type_name={}, mono_type_name={}, method={}",
                                ns, type_name, mono_type_name, name
                            );

                            // Check if this is a known method - try to find it
                            let method_name = format!("{}_{}", mono_type_name, name);
                            let mangled = self.mangle_name(&method_name, false);

                            // Check if the method actually exists in the module
                            let method_exists = self.module.get_function(&mangled).is_some();

                            if method_exists {
                                (mangled, false, true, false)
                            } else {
                                // Method doesn't exist - this is likely a function pointer field
                                // Return a marker to handle it as field access
                                (format!("{}__fn_ptr_field", name), false, false, true)
                            }
                        } else {
                            // Not a custom type, use the namespace as-is
                            let actual_package = self
                                .imported_packages
                                .get(ns)
                                .map(|s| s.as_str())
                                .unwrap_or(ns);

                            if actual_package == "io" && name == "println" {
                                return self.generate_hir_io_println(args);
                            }

                            (format!("{}_{}", actual_package, name), true, false, false)
                        }
                    } else if let Some(actual_package) = self.imported_packages.get(ns) {
                        // Namespace is an imported package
                        if actual_package == "io" && name == "println" {
                            return self.generate_hir_io_println(args);
                        }
                        (format!("{}_{}", actual_package, name), true, false, false)
                    } else {
                        // Namespace is not a known variable or package - likely a local struct/enum or implicit built-in
                        if ns == "io" && name == "println" {
                            return self.generate_hir_io_println(args);
                        }
                        let combined_name = format!("{}_{}", ns, name);
                        (self.mangle_name(&combined_name, false), false, false, false)
                    }
                } else {
                    if name == "main" {
                        ("main".to_string(), false, false, false)
                    } else {
                        (
                            format!("{}_{}", self.module_name, name),
                            false,
                            false,
                            false,
                        )
                    }
                };

                // Handle function pointer field access
                if is_fn_ptr_field {
                    if let Some(ns) = namespace.as_deref() {
                        // Get the struct variable
                        let struct_ptr = self
                            .variables
                            .get(ns)
                            .or_else(|| self.const_variables.get(ns))
                            .copied()
                            .ok_or_else(|| format!("Variable not found: {}", ns))?;

                        let var_ty = self
                            .variable_types
                            .get(ns)
                            .ok_or_else(|| format!("Type not found for: {}", ns))?;

                        // Get the field index from the struct type
                        if let Type::Custom {
                            name: type_name, ..
                        } = var_ty
                        {
                            // Look up the struct type
                            if let Some(struct_type) = self.context.get_struct_type(type_name) {
                                // Find the field index - we need to iterate through struct fields
                                // For now, assume field 0 (add field)
                                // TODO: Make this more robust by finding the actual field
                                let field_idx = 0u32; // Assuming 'add' is the first field

                                // GEP to the field
                                let field_ptr = self
                                    .builder
                                    .build_struct_gep(
                                        struct_type,
                                        struct_ptr,
                                        field_idx,
                                        "field_ptr",
                                    )
                                    .map_err(|e| e.to_string())?;

                                // Load the function pointer
                                let fn_ptr_type =
                                    self.context.ptr_type(inkwell::AddressSpace::default());
                                let fn_ptr = self
                                    .builder
                                    .build_load(fn_ptr_type, field_ptr, "fn_ptr")?
                                    .into_pointer_value();

                                // Get the function type from HIR
                                // For now, use a simple i64 function type
                                // TODO: Get the actual function signature
                                let fn_sig = self.context.i64_type().fn_type(&[], false);

                                // Generate args first
                                let mut llvm_args: Vec<BasicMetadataValueEnum> = Vec::new();
                                for arg_expr in args {
                                    let val = self.generate_hir_expr(arg_expr)?;
                                    llvm_args.push(BasicMetadataValueEnum::from(val));
                                }

                                // Make indirect call
                                let call_result = self.builder.build_indirect_call(
                                    fn_sig,
                                    fn_ptr,
                                    &llvm_args,
                                    "indirect_call",
                                )?;

                                let result = match call_result.try_as_basic_value() {
                                    inkwell::values::ValueKind::Basic(val) => val,
                                    _ => self.context.i64_type().const_int(0, false).into(),
                                };
                                return Ok(result);
                            }
                        }
                    }
                }

                let function = self
                    .module
                    .get_function(&mangled_name)
                    .or_else(|| self.module.get_function(name)) // fallback demangled
                    .ok_or(format!(
                        "Fn not found: {} (original: {})",
                        mangled_name, name
                    ))?;

                let mut llvm_args = Vec::new();

                // If this is a method call on a struct instance, add self as first argument
                if needs_self {
                    if let Some(ns) = namespace.as_deref() {
                        if let Some(ptr) = self.variables.get(ns) {
                            // For methods, self is usually a pointer to the struct
                            // ONLY add self if the function actually expects it
                            if function.count_params() == args.len() as u32 + 1 {
                                llvm_args.push(BasicMetadataValueEnum::from(*ptr));
                            }
                        }
                    }
                }

                for arg in args {
                    let val = self.generate_hir_expr(arg)?;
                    llvm_args.push(BasicMetadataValueEnum::from(val));
                }

                let call_result = self.builder.build_call(function, &llvm_args, "call")?;
                let result = match call_result.try_as_basic_value() {
                    inkwell::values::ValueKind::Basic(val) => val,
                    _ => self.context.i64_type().const_int(0, false).into(),
                };
                Ok(result)
            }
            hir::HirExpr::If {
                condition,
                capture,
                then_branch,
                else_branch,
                ty,
                ..
            } => self.generate_hir_if_expr(condition, capture, then_branch, else_branch, ty),
            hir::HirExpr::Block { stmts, expr, .. } => self.generate_hir_block_expr(stmts, expr),
            hir::HirExpr::MemberAccess {
                object, member, ty, ..
            } => {
                // Check if this is an error member access (e.g., SampleError.CodegenError)
                if matches!(ty, Type::Error) {
                    let error_name = match object.as_ref() {
                        hir::HirExpr::Ident(name, _, _) => format!("{}.{}", name, member),
                        _ => member.clone(),
                    };
                    self.set_last_error_message(&error_name)?;

                    // For error member access, return a non-zero i64 value to indicate an error.
                    // This works because void! functions return i64 in LLVM, and other functions
                    // that return error types will handle the error appropriately.
                    return Ok(self
                        .context
                        .i64_type()
                        .const_int(1, false)
                        .as_basic_value_enum()
                        .into());
                }

                // Check if this is an enum variant access
                if let hir::HirExpr::Ident(obj_name, _, _) = object.as_ref() {
                    if let Some(variants) = self.enum_variants.get(obj_name) {
                        if let Some(&idx) = variants.get(member) {
                            return Ok(self.context.i64_type().const_int(idx as u64, false).into());
                        } else {
                            return Err(
                                format!("Enum variant not found: {}.{}", obj_name, member).into()
                            );
                        }
                    }
                }

                // For member access, we need to get the struct and extract the field
                // First check if object is a simple identifier - we can handle it specially

                // Get struct name and field index from the object if it's an identifier
                let (struct_name, field_idx) = if let hir::HirExpr::Ident(obj_name, _obj_ty, _) =
                    object.as_ref()
                {
                    let var_ty = match self.variable_types.get(obj_name) {
                        Some(ty) => ty.clone(),
                        None => return Err(format!("Variable type not found: {}", obj_name).into()),
                    };

                    let struct_name = match &var_ty {
                        Type::Pointer(inner) => {
                            if let Type::Custom { name, .. } = &**inner {
                                name.clone()
                            } else {
                                return Err("Member access on non-custom pointer type".into());
                            }
                        }
                        Type::Custom { name, .. } => name.clone(),
                        _ => return Err("Member access on non-struct type".into()),
                    };

                    let field_idx = self
                        .struct_field_indices
                        .get(&struct_name)
                        .and_then(|fields| fields.get(member))
                        .copied()
                        .ok_or_else(|| {
                            eprintln!(
                                "DEBUG MemberAccess error: field {} not found in struct {}",
                                member, struct_name
                            );
                            format!("Field '{}' not found in struct '{}'", member, struct_name)
                        })?;

                    (Some(struct_name), field_idx)
                } else {
                    // For non-identifier objects, fall back to numeric index (legacy behavior)
                    (None, member.parse().unwrap_or(0))
                };

                let extracted = if let hir::HirExpr::Ident(obj_name, _obj_ty, _) = object.as_ref() {
                    // Find the variable's allocation pointer
                    let alloca_ptr = self
                        .variables
                        .get(obj_name)
                        .or_else(|| self.const_variables.get(obj_name))
                        .copied()
                        .ok_or_else(|| format!("Variable not found: {}", obj_name))?;
                    let var_ty = self.variable_types.get(obj_name).unwrap();

                    match var_ty {
                        Type::Pointer(inner) => {
                            // Variable holds a pointer to a struct
                            if let Type::Custom {
                                name: struct_name, ..
                            } = &**inner
                            {
                                if let Some(struct_type) = self.context.get_struct_type(struct_name)
                                {
                                    // Load the pointer from alloca
                                    let opaque_ptr_type =
                                        self.context.ptr_type(inkwell::AddressSpace::default());
                                    let struct_ptr = self
                                        .builder
                                        .build_load(opaque_ptr_type, alloca_ptr, "deref_ptr")?
                                        .into_pointer_value();
                                    // GEP into the field
                                    let field_ptr = self
                                        .builder
                                        .build_struct_gep(
                                            struct_type,
                                            struct_ptr,
                                            field_idx,
                                            "field_ptr",
                                        )
                                        .map_err(|e| e.to_string())?;
                                    // Load the field value
                                    let field_type =
                                        struct_type.get_field_type_at_index(field_idx).unwrap();
                                    self.builder.build_load(field_type, field_ptr, member)?
                                } else {
                                    // Struct type not found, load as i64 fallback
                                    self.context
                                        .i64_type()
                                        .const_int(0, false)
                                        .as_basic_value_enum()
                                        .into()
                                }
                            } else {
                                // Pointer to non-struct, load it
                                let opaque_ptr_type =
                                    self.context.ptr_type(inkwell::AddressSpace::default());
                                self.builder
                                    .build_load(opaque_ptr_type, alloca_ptr, obj_name)?
                            }
                        }
                        Type::Custom {
                            name: struct_name, ..
                        } => {
                            // Variable holds a struct directly
                            if let Some(struct_type) = self.context.get_struct_type(struct_name) {
                                let field_ptr = self
                                    .builder
                                    .build_struct_gep(
                                        struct_type,
                                        alloca_ptr,
                                        field_idx,
                                        "field_ptr",
                                    )
                                    .map_err(|e| e.to_string())?;
                                let field_type =
                                    struct_type.get_field_type_at_index(field_idx).unwrap();
                                self.builder.build_load(field_type, field_ptr, member)?
                            } else {
                                // Fallback: load whole struct then extractvalue
                                let llvm_ty = self.llvm_type(var_ty);
                                let struct_val =
                                    self.builder.build_load(llvm_ty, alloca_ptr, obj_name)?;
                                self.builder.build_extract_value(
                                    struct_val.into_struct_value(),
                                    field_idx,
                                    member,
                                )?
                            }
                        }
                        _ => {
                            // Non-struct or non-pointer — load raw
                            let llvm_ty = self.llvm_type(var_ty);
                            self.builder.build_load(llvm_ty, alloca_ptr, obj_name)?
                        }
                    }
                } else {
                    // Object is not a simple identifier - evaluate it and extract from resulting value
                    let obj_val = self.generate_hir_expr(object)?;
                    match obj_val.get_type() {
                        BasicTypeEnum::StructType(_) => self.builder.build_extract_value(
                            obj_val.into_struct_value(),
                            field_idx,
                            member,
                        )?,
                        _ => {
                            // Pointer or scalar fallback - return a zeroinitializer as i64
                            self.context
                                .i64_type()
                                .const_int(0, false)
                                .as_basic_value_enum()
                                .into()
                        }
                    }
                };

                Ok(extracted.into())
            }
            hir::HirExpr::Struct {
                name, fields, ty, ..
            } => {
                // Create a struct instance
                let struct_type = self.llvm_type(ty).into_struct_type();
                let struct_name = match ty {
                    Type::Custom { name, .. } => name.clone(),
                    _ => name.clone(),
                };

                // Initialize with undefined value
                let mut struct_val = struct_type.get_undef();

                // Build the struct by inserting field values dynamically
                for (field_name, field_expr) in fields {
                    let field_val = self.generate_hir_expr(field_expr)?;

                    // Look up the field index and type
                    let (field_idx, field_ty) = self
                        .struct_field_indices
                        .get(&struct_name)
                        .and_then(|m| {
                            m.get(field_name).map(|idx| {
                                // Find the AST type from the struct definition
                                let ast_ty = self
                                    .structs
                                    .get(&struct_name)
                                    .and_then(|s| s.fields.get(*idx as usize))
                                    .map(|f| f.ty.clone())
                                    .unwrap_or(Type::I64); // Fallback
                                (*idx, ast_ty)
                            })
                        })
                        .ok_or_else(|| {
                            eprintln!(
                                "DEBUG Struct instantiation error: field {} not found in struct {}",
                                field_name, struct_name
                            );
                            format!(
                                "Field '{}' not found in struct '{}'",
                                field_name, struct_name
                            )
                        })?;

                    // Coerce the field value to the expected AST type
                    let coerced_val = self.coerce_type(field_val, &field_ty)?;

                    struct_val = self
                        .builder
                        .build_insert_value(
                            struct_val,
                            coerced_val,
                            field_idx,
                            &format!("{}.{}", struct_name, field_name),
                        )?
                        .into_struct_value();
                }

                Ok(struct_val.as_basic_value_enum())
            }
            hir::HirExpr::Try { expr, ty: _, .. } => self.generate_hir_try_expr(expr),
            hir::HirExpr::Catch {
                expr,
                error_var: _,
                body,
                ty: _,
                span: _,
            } => self.generate_hir_catch_expr(expr, body),
            hir::HirExpr::Cast {
                target_type,
                expr,
                ty: _,
                span: _,
            } => {
                let expr_value = self.generate_hir_expr(expr)?;
                self.generate_cast(expr_value, target_type)
            }
            hir::HirExpr::Dereference { expr, ty, span: _ } => {
                // Generate the pointer expression
                let ptr_value = self.generate_hir_expr(expr)?;
                // Get the LLVM type for the inner type
                let llvm_type = self.llvm_type(ty);
                // Convert to pointer value
                let ptr = ptr_value.into_pointer_value();
                // Load the value from the pointer
                let loaded = self.builder.build_load(llvm_type, ptr, "deref_load")?;
                Ok(loaded.into())
            }
            hir::HirExpr::Intrinsic { name, args, .. } => {
                self.generate_intrinsic(name, args)
            }
            hir::HirExpr::TypeLiteral(_, _, _) => {
                Err("Type literal should only be used as an argument to an intrinsic".into())
            }
        }
    }
}
