use crate::builtin::Intrinsic;
use crate::codegen::{CodeGenerator, CodegenResult};
use crate::hir;
use inkwell::values::BasicValueEnum;

pub struct BitCastIntrinsic;

impl Intrinsic for BitCastIntrinsic {
    fn name(&self) -> &'static str {
        "@bit_cast"
    }

    fn generate<'ctx>(
        &self,
        codegen: &mut CodeGenerator<'ctx>,
        args: &[hir::HirExpr],
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        if args.len() != 2 {
            return Err("@bit_cast requires exactly two arguments: (value, Type)".into());
        }

        let value = codegen.generate_hir_expr(&args[0])?;
        let target_ty = match &args[1] {
            hir::HirExpr::TypeLiteral(ty, _, _) => ty,
            _ => return Err("Second argument to @bit_cast must be a type literal".into()),
        };

        let llvm_target_ty = codegen.llvm_type(target_ty);
        
        // Use appropriate LLVM instruction based on source and target types
        let bitcast_value = if value.is_int_value() && llvm_target_ty.is_pointer_type() {
            codegen.builder.build_int_to_ptr(
                value.into_int_value(),
                llvm_target_ty.into_pointer_type(),
                "int_to_ptr",
            )?
            .into()
        } else if value.is_pointer_value() && llvm_target_ty.is_int_type() {
            codegen.builder.build_ptr_to_int(
                value.into_pointer_value(),
                llvm_target_ty.into_int_type(),
                "ptr_to_int",
            )?
            .into()
        } else {
            codegen.builder.build_bit_cast(value, llvm_target_ty, "bitcast_tmp")?
        };
        
        Ok(bitcast_value)
    }
}
