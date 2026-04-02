use crate::builtin::Intrinsic;
use crate::codegen::{CodeGenerator, CodegenResult};
use crate::hir;
use inkwell::values::BasicValueEnum;

pub struct IsNotNullIntrinsic;

impl Intrinsic for IsNotNullIntrinsic {
    fn name(&self) -> &'static str {
        "@is_not_null"
    }

    fn generate<'ctx>(
        &self,
        codegen: &mut CodeGenerator<'ctx>,
        args: &[hir::HirExpr],
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        // Generate the argument (pointer value)
        let arg = codegen.generate_hir_expr(&args[0])?;

        // Get the pointer value
        let ptr_value = match arg {
            BasicValueEnum::PointerValue(ptr) => ptr,
            BasicValueEnum::IntValue(int_val) => {
                // Convert integer to pointer
                codegen.builder.build_int_to_ptr(
                    int_val,
                    codegen.context.ptr_type(inkwell::AddressSpace::default()),
                    "int_to_ptr",
                )?
            }
            _ => return Err("Invalid argument type for null check".into()),
        };

        // Compare with null (zero pointer)
        let ptr_as_int = codegen.builder.build_ptr_to_int(
            ptr_value,
            codegen.context.i64_type(),
            "ptr_to_int",
        )?;

        let zero = codegen.context.i64_type().const_int(0, false);

        let is_not_null = codegen.builder.build_int_compare(
            inkwell::IntPredicate::NE,
            ptr_as_int,
            zero,
            "ptr_is_not_null",
        )?;

        Ok(is_not_null.into())
    }
}
