//! Run command - executes Lang source file via JIT compiler

use std::collections::HashMap;
use std::error::Error;

use crate::codegen;
use crate::lower;
use crate::parser;
use crate::sema;
use crate::stdlib;

/// Run the compiled program (JIT)
pub fn run_jit(
    source: &str,
    cli_std_path: Option<std::path::PathBuf>,
) -> Result<(), Box<dyn Error>> {
    // Initialize std library
    println!("Loading std library...");
    let mut stdlib = stdlib::StdLib::new();
    let stdlib_path = crate::cmd::resolve_std_path(cli_std_path);
    stdlib.set_std_path(stdlib_path.to_str().unwrap());
    // Preload builtin package (contains is_null, is_not_null, etc.)
    let _ = stdlib.preload_builtins();
    // Don't preload packages - require explicit imports
    // let _ = stdlib.preload_common();
    println!(
        "Loaded std packages: {:?}",
        stdlib.packages().keys().collect::<Vec<_>>()
    );

    // Parse source code
    println!("Parsing source code...");
    let mut program = parser::parse(source)?;

    // Load imported packages
    println!("Loading imported packages...");
    for (_, package_name) in &program.imports {
        let _ = stdlib.load_package(package_name);
    }
    println!(
        "Loaded std packages: {:?}",
        stdlib.packages().keys().collect::<Vec<_>>()
    );

    // Semantic Analysis
    println!("Semantic Analysis...");
    let mut analyzer = sema::SemanticAnalyzer::new();
    analyzer.analyze_with_stdlib(&mut program, Some(&stdlib))?;

    // Generate LLVM IR
    let context = inkwell::context::Context::create();
    let typed_program = analyzer
        .get_typed_program()
        .ok_or("No typed program found")?;
    let mut monomorphized_structs = HashMap::new();
    for s in &typed_program.structs {
        monomorphized_structs.insert(s.name.clone(), s.clone());
    }

    let mut codegen = codegen::CodeGenerator::new(
        &context,
        "lang",
        stdlib,
        monomorphized_structs,
        analyzer.enums.clone(),
        analyzer.errors.clone(),
    )?;

    // Process imports (declares functions from imported packages)
    codegen.process_imports(&program.imports)?;

    let mut lowering_ctx = lower::LoweringContext::new();
    lowering_ctx.set_symbol_table(analyzer.get_symbol_table().clone());
    let typed_program = analyzer
        .get_typed_program()
        .ok_or("No typed program found")?;

    // Declare structs and enums first (needed for function return types)
    for s in &typed_program.structs {
        codegen.declare_struct(s)?;
    }
    for e in &program.enums {
        codegen.declare_enum(e)?;
    }

    for f in &typed_program.functions {
        codegen.declare_function(f)?;
    }

    let hir_program = lowering_ctx.lower_program(&program, typed_program);
    codegen.generate_hir(&hir_program)?;

    // Print the generated IR
    println!("\nGenerated LLVM IR:");
    println!("{}", codegen.print_ir());

    // Execute main function if it exists
    if program.functions.iter().any(|f| f.name == "main") {
        println!("\nExecuting main function via JIT...");
        // Note: For a simple i64 return, we would need more setup
        // This is a placeholder for JIT execution
    }

    Ok(())
}
