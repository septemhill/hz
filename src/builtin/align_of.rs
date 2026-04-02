use crate::builtin::Intrinsic;
use crate::codegen::{CodeGenerator, CodegenResult};
use crate::hir;
use inkwell::types::{BasicType, BasicTypeEnum};
use inkwell::values::BasicValueEnum;

pub struct AlignOfIntrinsic;

impl Intrinsic for AlignOfIntrinsic {
    fn name(&self) -> &'static str {
        "@align_of"
    }

    fn generate<'ctx>(
        &self,
        codegen: &mut CodeGenerator<'ctx>,
        args: &[hir::HirExpr],
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let ty = match &args[0] {
            hir::HirExpr::TypeLiteral(ty, _, _) => ty,
            _ => return Err("Argument to @align_of must be a type literal".into()),
        };

        let llvm_type = codegen.llvm_type(ty);

        let align = llvm_type.get_alignment();

        // Convert to u64 as requested
        let u64_type = codegen.context.i64_type();
        let align_u64 = codegen
            .builder
            .build_int_cast(align, u64_type, "align_of_u64")?;

        Ok(align_u64.into())
    }
}
