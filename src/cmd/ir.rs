//! IR command - generates LLVM IR only

use std::error::Error;

use crate::codegen;
use crate::lower;
use crate::parser;
use crate::stdlib;

/// Generate LLVM IR only
pub fn generate_ir(source: &str, output_path: Option<String>) -> Result<(), Box<dyn Error>> {
    // Initialize std library
    println!("Loading std library...");
    let mut stdlib = stdlib::StdLib::new();
    stdlib.set_std_path("./std");

    // Parse source code
    println!("Parsing source code...");
    let program = parser::parse(source)?;

    // Load imported packages
    println!("Loading imported packages...");
    for (_, package_name) in &program.imports {
        stdlib.load_package(package_name)?;
    }
    println!(
        "Loaded std packages: {:?}",
        stdlib.packages().keys().collect::<Vec<_>>()
    );

    // Generate LLVM IR
    let context = inkwell::context::Context::create();
    let mut codegen = codegen::CodeGenerator::new(&context, "lang", stdlib)?;

    // Process imports (declares functions from imported packages)
    codegen.process_imports(&program.imports)?;

    // Declare structs and enums
    for s in &program.structs {
        codegen.declare_struct(s)?;
    }
    for e in &program.enums {
        codegen.declare_enum(e)?;
    }

    // Declare functions
    for f in &program.functions {
        codegen.declare_function(f)?;
    }

    // Declare external C functions (FFI)
    for ext_fn in &program.external_functions {
        codegen.declare_c_function(ext_fn)?;
    }

    // Lower and generate
    let mut lowering_ctx = lower::LoweringContext::new();
    let hir_program = lowering_ctx.lower_program(&program);
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
