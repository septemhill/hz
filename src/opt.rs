use crate::hir;

pub fn optimize(program: &mut hir::HirProgram) {
    for f in &mut program.functions {
        optimize_fn(f);
    }
}

fn optimize_fn(f: &mut hir::HirFn) {
    // Basic constant folding or dead code elimination could go here
    // For now, it's a pass-through
}
