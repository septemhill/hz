use crate::builtin::Intrinsic;
use crate::codegen::{CodeGenerator, CodegenResult};
use crate::hir;
use inkwell::types::BasicType;
use inkwell::values::BasicValueEnum;

pub struct SizeOfIntrinsic;

impl Intrinsic for SizeOfIntrinsic {
    fn name(&self) -> &'static str {
        "@size_of"
    }

    fn generate<'ctx>(
        &self,
        codegen: &mut CodeGenerator<'ctx>,
        args: &[hir::HirExpr],
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let ty = match &args[0] {
            hir::HirExpr::TypeLiteral(ty, _, _) => ty,
            _ => return Err("Argument to @size_of must be a type literal".into()),
        };

        let llvm_type = codegen.llvm_type(ty);
        
        // size_of returns an i64 (or target dependent integer)
        let size = llvm_type.size_of().ok_or("Cannot get size of type")?;
        
        // Convert to u64 as requested
        let u64_type = codegen.context.i64_type();
        let size_u64 = codegen.builder.build_int_cast(size, u64_type, "size_of_u64")?;
        
        Ok(size_u64.into())
    }
}
