//! AST command - dumps the Abstract Syntax Tree

use std::error::Error;

use crate::ast::AstDump;
use crate::parser;
use crate::sema::SemanticAnalyzer;

/// Dump AST (Abstract Syntax Tree) with type information
pub fn dump_ast(source: &str) -> Result<(), Box<dyn Error>> {
    // Parse source code
    let program = parser::parse(source)?;

    // Run semantic analysis to get typed AST
    let mut analyzer = SemanticAnalyzer::new();
    match analyzer.analyze(&program) {
        Ok(_) => {
            if let Some(typed_program) = analyzer.get_typed_program() {
                typed_program.dump(0);
            } else {
                println!("Warning: Semantic analysis succeeded but no typed program was produced.");
                program.dump(0);
            }
        }
        Err(e) => {
            eprintln!("Semantic Analysis Error: {}", e);
            println!("Dumping untyped AST:");
            program.dump(0);
        }
    }

    Ok(())
}
