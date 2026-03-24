//! Run command - executes Lang source file via JIT compiler

use std::error::Error;

use crate::codegen;
use crate::lower;
use crate::parser;
use crate::sema;
use crate::stdlib;

/// Run the compiled program (JIT)
pub fn run_jit(source: &str, cli_std_path: Option<std::path::PathBuf>) -> Result<(), Box<dyn Error>> {
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
    let program = parser::parse(source)?;

    // Semantic Analysis
    println!("Semantic Analysis...");
    let mut analyzer = sema::SemanticAnalyzer::new();
    analyzer.analyze_with_stdlib(&program, Some(&stdlib))?;

    // Generate LLVM IR
    let context = inkwell::context::Context::create();
    let mut codegen = codegen::CodeGenerator::new(
        &context,
        "lang",
        stdlib,
        analyzer.structs.clone(),
        analyzer.enums.clone(),
        analyzer.errors.clone(),
    )?;

    let mut lowering_ctx = lower::LoweringContext::new();
    lowering_ctx.set_symbol_table(analyzer.get_symbol_table().clone());
    let hir_program = lowering_ctx.lower_program(&program);

    for f in &program.functions {
        codegen.declare_function(f)?;
    }

    // Declare structs and enums (needed for method calls)
    for s in &program.structs {
        codegen.declare_struct(s)?;
    }
    for e in &program.enums {
        codegen.declare_enum(e)?;
    }

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
