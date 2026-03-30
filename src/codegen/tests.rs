use super::*;
use crate::ast::{BinaryOp, Mutability, Span, Type};
use crate::hir::{HirExpr, HirStmt};
use crate::lower;
use crate::parser;
use crate::sema::SemanticAnalyzer;
use inkwell::context::Context;
use std::collections::HashMap;

/// Helper to compile Lang source code to LLVM IR
fn get_ir_from_source(
    context: &Context,
    source: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let stdlib = StdLib::new();
    let mut program = parser::parse(source)?;

    let mut analyzer = SemanticAnalyzer::new();
    analyzer.analyze_with_stdlib(&mut program, Some(&stdlib), true)?;

    let typed_program = analyzer
        .get_typed_program()
        .ok_or("No typed program found")?;
    let mut monomorphized_structs = HashMap::new();
    for s in &typed_program.structs {
        monomorphized_structs.insert(s.name.clone(), s.clone());
    }

    let mut codegen = CodeGenerator::new(
        context,
        "test_module",
        stdlib,
        monomorphized_structs,
        analyzer.enums.clone(),
        analyzer.errors.clone(),
    )?;

    // Declare functions
    for f in &typed_program.functions {
        codegen.declare_function(f)?;
    }
    for s in &typed_program.structs {
        codegen.declare_struct(s)?;
    }

    let mut lowering_ctx = lower::LoweringContext::new();
    lowering_ctx.set_symbol_table(analyzer.get_symbol_table().clone());
    let hir_program = lowering_ctx.lower_program(&program, typed_program);

    codegen.generate_hir(&hir_program)?;
    Ok(codegen.print_ir())
}

#[test]
fn test_ir_from_source_simple() -> Result<(), Box<dyn std::error::Error>> {
    let context = Context::create();
    let source = "fn main() i64 { return 42; }";
    let ir = get_ir_from_source(&context, source)?;

    assert!(ir.contains("define i64 @main()"));
    assert!(ir.contains("ret i64 42"));
    Ok(())
}

#[test]
fn test_ir_from_source_if_else() -> Result<(), Box<dyn std::error::Error>> {
    let context = Context::create();
    let source = "
    fn check(val: i64) i64 {
        if (val > 0) {
            return 1;
        } else {
            return 0;
        }
    }";
    let ir = get_ir_from_source(&context, source)?;

    assert!(ir.contains("define i64 @test_module_check(i64 %0)"));
    assert!(ir.contains("icmp ugt i64 %val1, 0"));
    assert!(ir.contains("br i1 %gt, label %then, label %else"));
    assert!(ir.contains("then:"));
    assert!(ir.contains("ret i64 1"));
    assert!(ir.contains("else:"));
    assert!(ir.contains("ret i64 0"));
    Ok(())
}

#[test]
fn test_ir_from_source_struct_method() -> Result<(), Box<dyn std::error::Error>> {
    let context = Context::create();
    let source = "
    struct Point {
        x: i64,
        y: i64,

        fn sum(self: Point) i64 {
            return self.x + self.y;
        }
    }

    fn test() i64 {
        const p: Point = Point{ x: 10, y: 20 };
        return p.sum();
    }";
    let ir = get_ir_from_source(&context, source)?;

    // Check struct declaration
    assert!(ir.contains("%Point = type { i64, i64 }"));
    // Check method declaration (mangled)
    assert!(ir.contains("define i64 @test_module_Point_sum("));
    // Check method call
    assert!(ir.contains("call i64 @test_module_Point_sum(ptr %p)"));

    Ok(())
}

#[test]
fn test_ir_from_source_for_loop() -> Result<(), Box<dyn std::error::Error>> {
    let context = Context::create();
    let source = "
    fn count(n: i64) i64 {
        var sum: i64 = 0;
        for (0..n) |i| {
            sum = sum + i;
        }
        return sum;
    }";
    let ir = get_ir_from_source(&context, source)?;

    assert!(ir.contains("define i64 @test_module_count(i64 %0)"));
    assert!(ir.contains("for_cond:"));
    assert!(ir.contains("icmp slt i64 %range_start, %range_end"));

    Ok(())
}

#[test]
fn test_generate_let_stmt() -> Result<(), Box<dyn std::error::Error>> {
    let context = Context::create();
    let stdlib = StdLib::new();
    let mut codegen = CodeGenerator::new(
        &context,
        "test_module",
        stdlib,
        HashMap::new(),
        HashMap::new(),
        HashMap::new(),
    )?;

    // Setup a dummy function to host the statement
    let i64_type = context.i64_type();
    let fn_type = i64_type.fn_type(&[], false);
    let function = codegen.module.add_function("test_fn", fn_type, None);
    let entry_block = context.append_basic_block(function, "entry");
    codegen.builder.position_at_end(entry_block);
    codegen.current_function = Some(function);
    codegen.current_block = Some(entry_block);

    // Create a Let statement: let x: i64 = 42;
    let let_stmt = HirStmt::Let {
        name: "x".to_string(),
        ty: Type::I64,
        value: Some(HirExpr::Int(42, Type::I64, Span::default())),
        mutability: Mutability::Var,
        span: Span::default(),
    };

    // Generate code for the statement
    codegen.generate_hir_stmt(&let_stmt)?;

    // Verify that the variable was added to the codegen's variable map
    assert!(codegen.variables.contains_key("x"));
    assert_eq!(codegen.variable_types.get("x").unwrap(), &Type::I64);

    // Check the generated IR
    let ir = codegen.print_ir();
    assert!(ir.contains("%x = alloca i64"));
    assert!(ir.contains("store i64 42, ptr %x"));

    Ok(())
}

#[test]
fn test_generate_assign_stmt() -> Result<(), Box<dyn std::error::Error>> {
    let context = Context::create();
    let stdlib = StdLib::new();
    let mut codegen = CodeGenerator::new(
        &context,
        "test_module",
        stdlib,
        HashMap::new(),
        HashMap::new(),
        HashMap::new(),
    )?;

    // Setup a dummy function
    let i64_type = context.i64_type();
    let fn_type = i64_type.fn_type(&[], false);
    let function = codegen.module.add_function("test_fn", fn_type, None);
    let entry_block = context.append_basic_block(function, "entry");
    codegen.builder.position_at_end(entry_block);
    codegen.current_function = Some(function);
    codegen.current_block = Some(entry_block);

    // 1. Declare x: let x: i64 = 10;
    let let_stmt = HirStmt::Let {
        name: "x".to_string(),
        ty: Type::I64,
        value: Some(HirExpr::Int(10, Type::I64, Span::default())),
        mutability: Mutability::Var,
        span: Span::default(),
    };
    codegen.generate_hir_stmt(&let_stmt)?;

    // 2. Assign x: x = 20;
    let assign_stmt = HirStmt::Assign {
        target: "x".to_string(),
        value: HirExpr::Int(20, Type::I64, Span::default()),
        span: Span::default(),
    };
    codegen.generate_hir_stmt(&assign_stmt)?;

    // Check the generated IR
    let ir = codegen.print_ir();
    assert!(ir.contains("store i64 10, ptr %x"));
    assert!(ir.contains("store i64 20, ptr %x"));

    Ok(())
}

#[test]
fn test_generate_if_stmt() -> Result<(), Box<dyn std::error::Error>> {
    let context = Context::create();
    let stdlib = StdLib::new();
    let mut codegen = CodeGenerator::new(
        &context,
        "test_module",
        stdlib,
        HashMap::new(),
        HashMap::new(),
        HashMap::new(),
    )?;

    // Setup a dummy function
    let i64_type = context.i64_type();
    let fn_type = i64_type.fn_type(&[], false);
    let function = codegen.module.add_function("test_fn", fn_type, None);
    let entry_block = context.append_basic_block(function, "entry");
    codegen.builder.position_at_end(entry_block);
    codegen.current_function = Some(function);
    codegen.current_block = Some(entry_block);
    codegen.return_type = Some(Type::I64);

    // if (true) { return 1; } else { return 0; }
    let if_stmt = HirStmt::If {
        condition: HirExpr::Bool(true, Type::Bool, Span::default()),
        capture: None,
        then_branch: Box::new(HirStmt::Return(
            Some(HirExpr::Int(1, Type::I64, Span::default())),
            Span::default(),
        )),
        else_branch: Some(Box::new(HirStmt::Return(
            Some(HirExpr::Int(0, Type::I64, Span::default())),
            Span::default(),
        ))),
        span: Span::default(),
    };

    codegen.generate_hir_stmt(&if_stmt)?;

    let ir = codegen.print_ir();
    assert!(ir.contains("br i1 true, label %then, label %else"));
    assert!(ir.contains("then:"));
    assert!(ir.contains("ret i64 1"));
    assert!(ir.contains("else:"));
    assert!(ir.contains("ret i64 0"));

    Ok(())
}

#[test]
fn test_generate_return_stmt() -> Result<(), Box<dyn std::error::Error>> {
    let context = Context::create();
    let stdlib = StdLib::new();
    let mut codegen = CodeGenerator::new(
        &context,
        "test_module",
        stdlib,
        HashMap::new(),
        HashMap::new(),
        HashMap::new(),
    )?;

    let i64_type = context.i64_type();
    let fn_type = i64_type.fn_type(&[], false);
    let function = codegen.module.add_function("test_fn", fn_type, None);
    let entry_block = context.append_basic_block(function, "entry");
    codegen.builder.position_at_end(entry_block);
    codegen.current_function = Some(function);
    codegen.current_block = Some(entry_block);
    codegen.return_type = Some(Type::I64);

    let ret_stmt = HirStmt::Return(
        Some(HirExpr::Int(123, Type::I64, Span::default())),
        Span::default(),
    );
    codegen.generate_hir_stmt(&ret_stmt)?;

    let ir = codegen.print_ir();
    assert!(ir.contains("ret i64 123"));
    Ok(())
}

#[test]
fn test_generate_member_assign_stmt() -> Result<(), Box<dyn std::error::Error>> {
    let context = Context::create();
    let stdlib = StdLib::new();

    let mut struct_field_indices = HashMap::new();
    let mut fields = HashMap::new();
    fields.insert("x".to_string(), 0);
    struct_field_indices.insert("Point".to_string(), fields);

    let mut codegen = CodeGenerator::new(
        &context,
        "test_module",
        stdlib,
        HashMap::new(),
        HashMap::new(),
        HashMap::new(),
    )?;
    codegen.struct_field_indices = struct_field_indices;

    let i64_type = context.i64_type();
    // Create named struct type for GEP
    let point_type = context.opaque_struct_type("Point");
    point_type.set_body(&[i64_type.into()], false);

    let fn_type = context.void_type().fn_type(&[], false);
    let function = codegen.module.add_function("test_fn", fn_type, None);
    let entry_block = context.append_basic_block(function, "entry");
    codegen.builder.position_at_end(entry_block);
    codegen.current_function = Some(function);
    codegen.current_block = Some(entry_block);

    // let p: Point = ...
    let p_alloca = codegen.builder.build_alloca(point_type, "p")?;
    codegen.variables.insert("p".to_string(), p_alloca);
    codegen.variable_types.insert(
        "p".to_string(),
        Type::Custom {
            name: "Point".to_string(),
            generic_args: vec![],
            is_exported: false,
        },
    );

    // p.x = 42
    let assign_stmt = HirStmt::Assign {
        target: "p.x".to_string(),
        value: HirExpr::Int(42, Type::I64, Span::default()),
        span: Span::default(),
    };

    codegen.generate_hir_stmt(&assign_stmt)?;

    let ir = codegen.print_ir();
    assert!(ir.contains("getelementptr"));
    assert!(ir.contains("store i64 42"));

    Ok(())
}

#[test]
fn test_generate_for_range_stmt() -> Result<(), Box<dyn std::error::Error>> {
    let context = Context::create();
    let stdlib = StdLib::new();
    let mut codegen = CodeGenerator::new(
        &context,
        "test_module",
        stdlib,
        HashMap::new(),
        HashMap::new(),
        HashMap::new(),
    )?;

    let i64_type = context.i64_type();
    let fn_type = context.void_type().fn_type(&[], false);
    let function = codegen.module.add_function("test_fn", fn_type, None);
    let entry_block = context.append_basic_block(function, "entry");
    codegen.builder.position_at_end(entry_block);
    codegen.current_function = Some(function);
    codegen.current_block = Some(entry_block);
    codegen.return_type = Some(Type::Void);

    // for (i in 0..10) {}
    let for_stmt = HirStmt::For {
        label: None,
        var_name: Some("i".to_string()),
        index_var: None,
        iterable: HirExpr::Binary {
            op: BinaryOp::Range,
            left: Box::new(HirExpr::Int(0, Type::I64, Span::default())),
            right: Box::new(HirExpr::Int(10, Type::I64, Span::default())),
            ty: Type::Tuple(vec![Type::I64, Type::I64]),
            span: Span::default(),
        },
        body: Box::new(HirStmt::Expr(HirExpr::Int(0, Type::I64, Span::default()))), // dummy body
        span: Span::default(),
    };

    codegen.generate_hir_stmt(&for_stmt)?;

    let ir = codegen.print_ir();
    assert!(ir.contains("for_eval:"));
    assert!(ir.contains("for_cond:"));
    assert!(ir.contains("icmp slt i64 %range_start, %range_end"));
    Ok(())
}

#[test]
fn test_generate_continue_uses_loop_latch() -> Result<(), Box<dyn std::error::Error>> {
    let context = Context::create();
    let stdlib = StdLib::new();
    let mut codegen = CodeGenerator::new(
        &context,
        "test_module",
        stdlib,
        HashMap::new(),
        HashMap::new(),
        HashMap::new(),
    )?;

    let fn_type = context.void_type().fn_type(&[], false);
    let function = codegen.module.add_function("test_fn", fn_type, None);
    let entry_block = context.append_basic_block(function, "entry");
    codegen.builder.position_at_end(entry_block);
    codegen.current_function = Some(function);
    codegen.current_block = Some(entry_block);
    codegen.return_type = Some(Type::Void);

    let for_stmt = HirStmt::For {
        label: None,
        var_name: Some("i".to_string()),
        index_var: None,
        iterable: HirExpr::Binary {
            op: BinaryOp::Range,
            left: Box::new(HirExpr::Int(1, Type::I64, Span::default())),
            right: Box::new(HirExpr::Int(5, Type::I64, Span::default())),
            ty: Type::Tuple(vec![Type::I64, Type::I64]),
            span: Span::default(),
        },
        body: Box::new(HirStmt::If {
            condition: HirExpr::Binary {
                op: BinaryOp::Gt,
                left: Box::new(HirExpr::Ident("i".to_string(), Type::I64, Span::default())),
                right: Box::new(HirExpr::Int(3, Type::I64, Span::default())),
                ty: Type::Bool,
                span: Span::default(),
            },
            capture: None,
            then_branch: Box::new(HirStmt::Continue {
                label: None,
                span: Span::default(),
            }),
            else_branch: None,
            span: Span::default(),
        }),
        span: Span::default(),
    };

    codegen.generate_hir_stmt(&for_stmt)?;

    let ir = codegen.print_ir();
    assert!(ir.contains("for_continue:"));
    assert!(ir.contains("then:"));
    assert!(ir.contains("br label %for_continue"));
    assert!(ir.contains("start_inc = add i64"));

    Ok(())
}

#[test]
fn test_generate_option_for_re_evaluates_iterable_and_stops_on_null()
-> Result<(), Box<dyn std::error::Error>> {
    let context = Context::create();
    let stdlib = StdLib::new();
    let mut codegen = CodeGenerator::new(
        &context,
        "test_module",
        stdlib,
        HashMap::new(),
        HashMap::new(),
        HashMap::new(),
    )?;

    let fn_type = context.void_type().fn_type(&[], false);
    let function = codegen.module.add_function("test_fn", fn_type, None);
    let entry_block = context.append_basic_block(function, "entry");
    codegen.builder.position_at_end(entry_block);
    codegen.current_function = Some(function);
    codegen.current_block = Some(entry_block);
    codegen.return_type = Some(Type::Void);

    let option_i64_type = context.struct_type(
        &[context.i64_type().into(), context.bool_type().into()],
        false,
    );
    let next_fn_type = option_i64_type.fn_type(&[], false);
    codegen
        .module
        .add_function("test_module_next", next_fn_type, None);

    let for_stmt = HirStmt::For {
        label: None,
        var_name: Some("item".to_string()),
        index_var: None,
        iterable: HirExpr::Call {
            name: "next".to_string(),
            namespace: None,
            args: vec![],
            return_ty: Type::Option(Box::new(Type::I64)),
            target_ty: None,
            span: Span::default(),
        },
        body: Box::new(HirStmt::Expr(HirExpr::Int(0, Type::I64, Span::default()))),
        span: Span::default(),
    };

    codegen.generate_hir_stmt(&for_stmt)?;

    let ir = codegen.print_ir();
    let for_eval_pos = ir.find("for_eval:").ok_or("missing for_eval block")?;
    let call_pos = ir
        .find("call { i64, i1 } @test_module_next()")
        .ok_or("missing next() call in IR")?;
    assert!(call_pos > for_eval_pos);
    assert_eq!(
        ir.matches("call { i64, i1 } @test_module_next()").count(),
        1
    );
    assert!(ir.contains("br i1 %is_null, label %for_end, label %for_body"));

    Ok(())
}

#[test]
fn test_generate_switch_stmt() -> Result<(), Box<dyn std::error::Error>> {
    let context = Context::create();
    let stdlib = StdLib::new();
    let mut codegen = CodeGenerator::new(
        &context,
        "test_module",
        stdlib,
        HashMap::new(),
        HashMap::new(),
        HashMap::new(),
    )?;

    let i64_type = context.i64_type();
    let fn_type = i64_type.fn_type(&[], false);
    let function = codegen.module.add_function("test_fn", fn_type, None);
    let entry_block = context.append_basic_block(function, "entry");
    codegen.builder.position_at_end(entry_block);
    codegen.current_function = Some(function);
    codegen.current_block = Some(entry_block);
    codegen.return_type = Some(Type::I64);

    // 1. Declare x: let x: i64 = 1;
    let let_stmt = HirStmt::Let {
        name: "x".to_string(),
        ty: Type::I64,
        value: Some(HirExpr::Int(1, Type::I64, Span::default())),
        mutability: Mutability::Var,
        span: Span::default(),
    };
    codegen.generate_hir_stmt(&let_stmt)?;

    // switch (x) { 1 => return 10, _ => return 20 }
    let switch_stmt = HirStmt::Switch {
        condition: HirExpr::Ident("x".to_string(), Type::I64, Span::default()),
        cases: vec![
            crate::hir::HirCase {
                patterns: vec![HirExpr::Int(1, Type::I64, Span::default())],
                body: HirStmt::Return(
                    Some(HirExpr::Int(10, Type::I64, Span::default())),
                    Span::default(),
                ),
                span: Span::default(),
            },
            crate::hir::HirCase {
                patterns: vec![HirExpr::Ident("_".to_string(), Type::I64, Span::default())],
                body: HirStmt::Return(
                    Some(HirExpr::Int(20, Type::I64, Span::default())),
                    Span::default(),
                ),
                span: Span::default(),
            },
        ],
        span: Span::default(),
    };

    codegen.generate_hir_stmt(&switch_stmt)?;

    let ir = codegen.print_ir();
    assert!(ir.contains("case_body:"));
    assert!(ir.contains("next_case:"));
    assert!(ir.contains("switch_end:"));
    assert!(ir.contains("icmp eq i64 %x1, 1")); // %x1 is the loaded value from %x
    Ok(())
}

#[test]
fn test_generate_break_stmt() -> Result<(), Box<dyn std::error::Error>> {
    let context = Context::create();
    let stdlib = StdLib::new();
    let mut codegen = CodeGenerator::new(
        &context,
        "test_module",
        stdlib,
        HashMap::new(),
        HashMap::new(),
        HashMap::new(),
    )?;

    let fn_type = context.void_type().fn_type(&[], false);
    let function = codegen.module.add_function("test_fn", fn_type, None);
    let entry_block = context.append_basic_block(function, "entry");
    codegen.builder.position_at_end(entry_block);
    codegen.current_function = Some(function);
    codegen.current_block = Some(entry_block);
    codegen.return_type = Some(Type::Void);

    // We need to setup loop_end_blocks to test break
    let end_block = context.append_basic_block(function, "loop_end");
    codegen.loop_end_blocks.push(vec![(end_block, None)]);

    let break_stmt = HirStmt::Break {
        label: None,
        span: Span::default(),
    };
    codegen.generate_hir_stmt(&break_stmt)?;

    let ir = codegen.print_ir();
    assert!(ir.contains("br label %loop_end"));

    Ok(())
}
