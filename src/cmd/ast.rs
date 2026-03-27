//! AST command - dumps the Abstract Syntax Tree

use std::error::Error;

use crate::ast::AstDump;
use crate::parser;
use crate::sema::SemanticAnalyzer;

/// Dump AST (Abstract Syntax Tree) with type information
pub fn dump_ast(
    source: &str,
    _cli_std_path: Option<std::path::PathBuf>,
) -> Result<(), Box<dyn Error>> {
    // Parse source code
    let program = parser::parse(source)?;

    // Initialize std library
    let mut stdlib = crate::stdlib::StdLib::new();
    let stdlib_path = crate::cmd::resolve_std_path(_cli_std_path);
    stdlib.set_std_path(stdlib_path.to_str().unwrap());
    let _ = stdlib.preload_builtins();

    // Load imported packages
    for (_, package_name) in &program.imports {
        let _ = stdlib.load_package(package_name);
    }

    // Run semantic analysis to get typed AST
    let mut analyzer = SemanticAnalyzer::new();
    match analyzer.analyze_with_stdlib(&program, Some(&stdlib)) {
        Ok(_) => {
            if let Some(typed_program) = analyzer.get_typed_program() {
                typed_program.dump(0);
            } else {
                println!("Warning: Semantic analysis succeeded but no typed program was produced.");
                program.dump(0);
            }
        }
        Err(e) => {
            if let Some(typed_program) = analyzer.get_typed_program() {
                eprintln!("Semantic Analysis Warning: {}", e);
                typed_program.dump(0);
            } else {
                eprintln!("Semantic Analysis Error: {}", e);
                println!("Dumping untyped AST:");
                program.dump(0);
            }
        }
    }

    Ok(())
}
