use super::*;
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum};

impl<'ctx> CodeGenerator<'ctx> {
    /// Entry point for preparing call arguments.
    /// Dispatches to either Lang or External C ABI preparation.
    pub(super) fn prepare_call_args(
        &mut self,
        function: FunctionValue<'ctx>,
        args: &[hir::HirExpr],
    ) -> CodegenResult<Vec<BasicMetadataValueEnum<'ctx>>> {
        if function.get_type().is_var_arg() {
            // C-style varargs functions (external cdecl)
            self.prepare_external_c_call_args(function, args)
        } else {
            // Regular Lang functions (including monomorphized varargs)
            self.prepare_lang_call_args(args)
        }
    }

    /// Prepare arguments for an internal Lang function call.
    fn prepare_lang_call_args(
        &mut self,
        args: &[hir::HirExpr],
    ) -> CodegenResult<Vec<BasicMetadataValueEnum<'ctx>>> {
        let mut llvm_args = Vec::new();
        for arg in args {
            // build_call_arg_from_hir_expr handles the Lang convention:
            // - Regular types: pass as-is
            // - VarArgsPack: alloca and pass as pointer to struct
            llvm_args.push(self.build_call_arg_from_hir_expr(arg)?);
        }
        Ok(llvm_args)
    }

    /// Prepare arguments for an External C FFI call.
    /// Handles C-style varargs unpacking and type promotion.
    fn prepare_external_c_call_args(
        &mut self,
        function: FunctionValue<'ctx>,
        args: &[hir::HirExpr],
    ) -> CodegenResult<Vec<BasicMetadataValueEnum<'ctx>>> {
        let mut llvm_args = Vec::new();
        let param_count = function.count_params();

        for (i, arg) in args.iter().enumerate() {
            let arg_ty = arg.ty();
            let is_vararg_pos = (i as u32) >= param_count;

            if let Type::VarArgsPack(types) = arg_ty {
                if is_vararg_pos {
                    // Unpack VarArgsPack into individual C arguments
                    let pack_val = self.generate_hir_expr(arg)?;
                    let struct_val = pack_val.into_struct_value();

                    for (idx, ty) in types.iter().enumerate() {
                        let val = self.builder.build_extract_value(
                            struct_val,
                            idx as u32,
                            "pack_extract",
                        )?;
                        let converted = self.apply_ffi_conversion(val, ty)?;
                        let promoted = self.promote_ffi_arg(converted, ty)?;
                        llvm_args.push(promoted.into());
                    }
                } else {
                    // This case shouldn't happen with current Sema logic for external fns,
                    // but for safety, we treat it as a pointer if it's in a fixed param position.
                    llvm_args.push(self.build_call_arg_from_hir_expr(arg)?);
                }
            } else {
                let val = self.generate_hir_expr(arg)?;
                // Apply FFI conversion (e.g. Slice -> Pointer)
                let converted = self.apply_ffi_conversion(val, arg_ty)?;

                if is_vararg_pos {
                    // Apply C promotion rules for variadic arguments
                    let promoted = self.promote_ffi_arg(converted, arg_ty)?;
                    llvm_args.push(promoted.into());
                } else {
                    llvm_args.push(converted.into());
                }
            }
        }
        Ok(llvm_args)
    }

    /// FFI-specific type conversion (e.g., Lang Slice -> C Pointer)
    fn apply_ffi_conversion(
        &self,
        value: BasicValueEnum<'ctx>,
        ty: &Type,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        match ty {
            Type::Array { size: None, .. } => {
                // Slices are { ptr, len }. Extract the ptr for C.
                if value.is_struct_value() {
                    Ok(self.builder.build_extract_value(
                        value.into_struct_value(),
                        0,
                        "slice_ptr_ffi",
                    )?)
                } else {
                    Ok(value)
                }
            }
            _ => Ok(value),
        }
    }

    /// C Variadic Promotion rules:
    /// - Signed types smaller than int (i8, i16) are sign-extended to i32.
    /// - Unsigned types smaller than int (u8, u16) are zero-extended to i32.
    /// - float (32-bit) is promoted to double (64-bit).
    fn promote_ffi_arg(
        &self,
        value: BasicValueEnum<'ctx>,
        ty: &Type,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        match ty {
            Type::I8 | Type::I16 => {
                let int_val = value.into_int_value();
                Ok(self
                    .builder
                    .build_int_s_extend_or_bit_cast(
                        int_val,
                        self.context.i32_type(),
                        "ffi_promoted_int",
                    )?
                    .into())
            }
            Type::U8 | Type::U16 | Type::Bool => {
                let int_val = value.into_int_value();
                Ok(self
                    .builder
                    .build_int_z_extend_or_bit_cast(
                        int_val,
                        self.context.i32_type(),
                        "ffi_promoted_int",
                    )?
                    .into())
            }
            Type::F32 => {
                let float_val = value.into_float_value();
                Ok(self
                    .builder
                    .build_float_ext(float_val, self.context.f64_type(), "ffi_promoted_double")?
                    .into())
            }
            _ => Ok(value),
        }
    }
}
