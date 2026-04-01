use crate::codegen::{CodeGenerator, CodegenResult};
use crate::hir;
use inkwell::values::BasicValueEnum;
use crate::builtin::Intrinsic;

pub struct IsNullIntrinsic;

impl Intrinsic for IsNullIntrinsic {
    fn name(&self) -> &'static str {
        "@is_null"
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
        let null_ptr = codegen
            .context
            .ptr_type(inkwell::AddressSpace::default())
            .const_null();
        let is_null = codegen.builder.build_int_compare(
            inkwell::IntPredicate::EQ,
            ptr_value,
            null_ptr,
            "ptr_is_null",
        )?;

        Ok(is_null.into())
    }
}
