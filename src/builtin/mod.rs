use crate::codegen::{CodeGenerator, CodegenResult};
use crate::hir;
use inkwell::values::BasicValueEnum;

pub mod is_null;
pub mod is_not_null;

use std::rc::Rc;

pub trait Intrinsic {
    fn name(&self) -> &'static str;
    fn generate<'ctx>(
        &self,
        codegen: &mut CodeGenerator<'ctx>,
        args: &[hir::HirExpr],
    ) -> CodegenResult<BasicValueEnum<'ctx>>;
}

pub fn get_intrinsics() -> Vec<Rc<dyn Intrinsic>> {
    vec![
        Rc::new(is_null::IsNullIntrinsic),
        Rc::new(is_not_null::IsNotNullIntrinsic),
    ]
}
