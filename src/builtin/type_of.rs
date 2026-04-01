use crate::builtin::Intrinsic;
use crate::codegen::{CodeGenerator, CodegenResult};
use crate::hir;
use inkwell::types::BasicType;
use inkwell::values::BasicValue;
use inkwell::values::BasicValueEnum;

pub struct TypeOfIntrinsic;

impl Intrinsic for TypeOfIntrinsic {
    fn name(&self) -> &'static str {
        "@type_of"
    }

    fn generate<'ctx>(
        &self,
        codegen: &mut CodeGenerator<'ctx>,
        args: &[hir::HirExpr],
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        // Get the type of the argument using the ty() method
        let arg_ty = args[0].ty();

        // Convert the type to a string representation
        let type_str = arg_ty.to_string();

        // Create a global string constant
        let str_val = unsafe {
            codegen
                .builder
                .build_global_string(&type_str, "type_of_str")
        }?;

        // Get the pointer value from the global string
        let ptr = str_val.as_basic_value_enum();
        let len = codegen
            .context
            .i64_type()
            .const_int(type_str.len() as u64, false);

        // Create the slice type { ptr: *const u8, len: i64 }
        let u8_ptr = codegen.context.ptr_type(inkwell::AddressSpace::default());
        let slice_type = codegen
            .context
            .struct_type(&[u8_ptr.into(), codegen.context.i64_type().into()], false);

        let mut slice_val = slice_type.get_undef();
        slice_val = codegen
            .builder
            .build_insert_value(slice_val, ptr, 0, "slice_ptr")?
            .into_struct_value();
        slice_val = codegen
            .builder
            .build_insert_value(slice_val, len, 1, "slice_len")?
            .into_struct_value();

        Ok(slice_val.into())
    }
}
