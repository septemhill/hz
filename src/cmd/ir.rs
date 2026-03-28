//! IR command - generates LLVM IR only

use std::collections::HashMap;
use std::error::Error;

use crate::codegen;
use crate::lower;
use crate::parser;
use crate::stdlib;

/// Generate LLVM IR only
pub fn generate_ir(
    source: &str,
    output_path: Option<String>,
    cli_std_path: Option<std::path::PathBuf>,
) -> Result<(), Box<dyn Error>> {
    // Initialize std library
    println!("Loading std library...");
    let mut stdlib = stdlib::StdLib::new();
    let stdlib_path = crate::cmd::resolve_std_path(cli_std_path);
    stdlib.set_std_path(stdlib_path.to_str().unwrap());

    // Parse source code
    println!("Parsing source code...");
    let mut program = parser::parse(source)?;

    // Load imported packages
    println!("Loading imported packages...");
    for (_, package_name) in &program.imports {
        stdlib.load_package(package_name)?;
    }
    println!(
        "Loaded std packages: {:?}",
        stdlib.packages().keys().collect::<Vec<_>>()
    );

    // Run semantic analysis to collect definitions
    let mut analyzer = crate::sema::SemanticAnalyzer::new();
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

    // Declare functions
    for f in &typed_program.functions {
        codegen.declare_function(f)?;
    }

    // Declare structs and enums (needed for method calls)
    for s in &typed_program.structs {
        codegen.declare_struct(s)?;
    }
    for e in &program.enums {
        codegen.declare_enum(e)?;
    }

    // Declare external C functions (FFI)
    for ext_fn in &program.external_functions {
        codegen.declare_c_function(ext_fn)?;
    }

    // Lower and generate
    let mut lowering_ctx = lower::LoweringContext::new();
    let hir_program = lowering_ctx.lower_program(&program, typed_program);
    codegen.generate_hir(&hir_program)?;
    let ir = codegen.print_ir();

    // Output IR
    if let Some(ref path) = output_path {
        std::fs::write(path, &ir)?;
        println!("LLVM IR written to {}", path);
    } else {
        println!("{}", ir);
    }

    Ok(())
}
