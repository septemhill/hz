//! HIR command - dumps the High-level Intermediate Representation

use std::error::Error;

use crate::lower;
use crate::parser;
use crate::sema;
use crate::stdlib;

/// Dump HIR (High-level Intermediate Representation)
pub fn dump_hir(
    source: &str,
    output_path: Option<String>,
    cli_std_path: Option<std::path::PathBuf>,
    enable_tree_shaking: bool,
) -> Result<(), Box<dyn Error>> {
    // Initialize std library
    println!("Loading std library...");
    let mut stdlib = stdlib::StdLib::new();
    let stdlib_path = crate::cmd::resolve_std_path(cli_std_path);
    stdlib.set_std_path(stdlib_path.to_str().unwrap());
    println!(
        "Loaded std packages: {:?}",
        stdlib.packages().keys().collect::<Vec<_>>()
    );

    // Parse source code
    println!("Parsing source code...");
    let mut program = parser::parse(source)?;
    println!("    Found {} function(s)", program.functions.len());

    // Semantic Analysis
    println!("Semantic Analysis...");
    let mut analyzer = sema::SemanticAnalyzer::new();
    analyzer.analyze_with_stdlib(&mut program, Some(&stdlib), enable_tree_shaking)?;

    // Lower to HIR
    println!("Lowering to HIR...");
    let mut lowering_ctx = lower::LoweringContext::new();
    lowering_ctx.set_symbol_table(analyzer.get_symbol_table().clone());
    let typed_program = analyzer
        .get_typed_program()
        .ok_or("No typed program found")?;
    let hir_program = lowering_ctx.lower_program(&program, typed_program);

    // Format HIR using Display (pretty print)
    let hir_pretty = hir_program.to_string();

    // Output HIR
    if let Some(ref path) = output_path {
        std::fs::write(path, &hir_pretty)?;
        println!("HIR written to {}", path);
    } else {
        println!("{}", hir_pretty);
    }

    Ok(())
}
