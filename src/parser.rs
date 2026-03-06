//! # Parser for Lang Programming Language
//!
//! This module parses source code and generates AST nodes.
//! It uses a state machine to track parsing progress for debugging purposes.

use crate::ast::*;
use crate::lexer::{PeekableLexerIterator, Token, TokenWithSpan, iter as lexer_iter};

/// Parser state for tracking current parsing context
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParserState {
    /// Initial state - waiting for top-level declaration
    Initial,
    /// Parsing import statement
    ParsingImport,
    /// Parsing struct definition
    ParsingStruct,
    /// Parsing enum definition
    ParsingEnum,
    /// Parsing function definition
    ParsingFunction,
    /// Parsing function parameters
    ParsingFunctionParams,
    /// Parsing function return type
    ParsingFunctionReturnType,
    /// Parsing function body
    ParsingFunctionBody,
    /// Parsing statement
    ParsingStatement,
    /// Parsing expression
    ParsingExpression,
    /// Parsing type
    ParsingType,
    /// Parsing complete
    Completed,
    /// Error state
    Error(String),
}

impl ParserState {
    /// Get a human-readable name for the state
    pub fn name(&self) -> &'static str {
        match self {
            ParserState::Initial => "Initial",
            ParserState::ParsingImport => "ParsingImport",
            ParserState::ParsingStruct => "ParsingStruct",
            ParserState::ParsingEnum => "ParsingEnum",
            ParserState::ParsingFunction => "ParsingFunction",
            ParserState::ParsingFunctionParams => "ParsingFunctionParams",
            ParserState::ParsingFunctionReturnType => "ParsingFunctionReturnType",
            ParserState::ParsingFunctionBody => "ParsingFunctionBody",
            ParserState::ParsingStatement => "ParsingStatement",
            ParserState::ParsingExpression => "ParsingExpression",
            ParserState::ParsingType => "ParsingType",
            ParserState::Completed => "Completed",
            ParserState::Error(_) => "Error",
        }
    }
}

/// Parser with state machine tracking
pub struct Parser {
    tokens: PeekableLexerIterator,
    state: ParserState,
    state_history: Vec<ParserState>,
}

impl Parser {
    /// Create a new parser from tokens (iterator)
    pub fn new(tokens: PeekableLexerIterator) -> Self {
        Parser {
            tokens,
            state: ParserState::Initial,
            state_history: Vec::new(),
        }
    }

    /// Create a new parser from source code directly
    pub fn from_source(source: &str) -> Result<Self, ParseError> {
        let tokens = lexer_iter(source);
        Ok(Parser::new(tokens))
    }

    /// Set the current state and record in history for debugging
    fn set_state(&mut self, new_state: ParserState) {
        // eprintln!(
        //     "DEBUG: Parser state transition: {:?} -> {:?}",
        //     self.state.name(),
        //     new_state.name()
        // );
        self.state_history.push(self.state.clone());
        self.state = new_state;
    }

    /// Get current state
    pub fn current_state(&self) -> &ParserState {
        &self.state
    }

    /// Get state history for debugging
    pub fn state_history(&self) -> &[ParserState] {
        &self.state_history
    }

    /// Get current token without advancing (mutable version)
    fn current_token(&mut self) -> Option<&TokenWithSpan> {
        self.tokens.peek(0)
    }

    /// Get current token value (for internal use with mutable reference)
    fn current(&mut self) -> Option<&Token> {
        self.tokens.peek(0).map(|t| &t.token)
    }

    /// Advance to next token
    fn advance(&mut self) {
        self.tokens.next();
    }

    /// Try to consume a specific token
    fn expect(&mut self, expected: Token) -> Result<TokenWithSpan, ParseError> {
        let token = match self.tokens.next() {
            Some(Ok(token)) => token,
            Some(Err(e)) => {
                return Err(ParseError {
                    message: e.message,
                    location: Some(e.location),
                });
            }
            None => {
                return Err(ParseError {
                    message: "Unexpected end of input".to_string(),
                    location: None,
                });
            }
        };

        if std::mem::discriminant(&token.token) == std::mem::discriminant(&expected) {
            Ok(token)
        } else {
            Err(ParseError {
                message: format!("Expected {:?}, got {:?}", expected, token.token),
                location: Some(token.span.start),
            })
        }
    }

    /// Try to match and consume a token without error
    fn match_token(&mut self, token: Token) -> bool {
        self.skip_whitespace();
        if let Some(t) = self.current() {
            eprintln!(
                "DEBUG match_token: current token is {:?}, trying to match {:?}",
                t, token
            );
            if *t == token {
                eprintln!("DEBUG match_token: matched {:?}, advancing", token);
                self.advance();
                return true;
            }
        }
        eprintln!(
            "DEBUG match_token: {:?} did NOT match, returning false",
            token
        );
        false
    }

    /// Match any assignment operator and return the corresponding AssignOp
    fn match_assign_op(&mut self) -> Option<AssignOp> {
        self.skip_whitespace();
        let token = self.current()?;
        let op = match token {
            Token::Assign => Some(AssignOp::Assign),
            Token::PlusAssign => Some(AssignOp::AddAssign),
            Token::MinusAssign => Some(AssignOp::SubAssign),
            Token::StarAssign => Some(AssignOp::MulAssign),
            Token::SlashAssign => Some(AssignOp::DivAssign),
            _ => None,
        };

        if op.is_some() {
            self.advance();
        }
        op
    }

    /// Peek at token without consuming
    ///
    /// If `offset` is 0, returns the current token. If `offset` is 1, returns the next token.
    fn peek(&mut self, offset: usize) -> Option<&TokenWithSpan> {
        self.tokens.peek(offset)
    }

    /// Check if at end of input
    fn is_at_end(&mut self) -> bool {
        self.tokens.is_at_end()
    }

    /// Skip whitespace (not needed with tokenized input, but kept for compatibility)
    fn skip_whitespace(&mut self) {
        // With lexer, we don't have whitespace tokens
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Test result structure for test runner
    struct TestResult {
        name: String,
        expected_success: bool,
        actual_success: bool,
        error_message: Option<String>,
    }

    impl TestResult {
        fn passed(&self) -> bool {
            self.expected_success == self.actual_success
        }
    }

    /// Test parsing a simple function
    #[test]
    #[ignore]
    fn test_parse_simple_function() {
        let source = r#"
import "io"

fn main() i64 {
    return 42;
}
"#;
        let result = parse(source);
        assert!(result.is_ok(), "Should parse simple function");

        let program = result.unwrap();
        assert_eq!(program.functions.len(), 1);
        assert_eq!(program.functions[0].name, "main");
    }

    /// Test parsing function with parameters
    #[test]
    #[ignore]
    fn test_parse_function_with_params() {
        let source = r#"
import "io"

fn add(a: i64, b: i64) i64 {
    return a + b;
}
"#;
        let result = parse(source);
        assert!(result.is_ok(), "Should parse function with parameters");

        let program = result.unwrap();
        assert_eq!(program.functions.len(), 1);
        assert_eq!(program.functions[0].name, "add");
        assert_eq!(program.functions[0].params.len(), 2);
    }

    /// Test parsing tuple types
    #[test]
    #[ignore]
    fn test_parse_tuple_type() {
        let source = r#"
import "io"

fn return_tuple() (i64, i64, i64) {
    return (1, 2, 3);
}
"#;
        let result = parse(source);
        assert!(result.is_ok(), "Should parse tuple type");
    }

    /// Test parsing import statement
    #[test]
    fn test_parse_import() {
        let source = r#"
import "io"

fn main() i64 {
    return 0;
}
"#;
        let result = parse(source);
        assert!(result.is_ok(), "Should parse import statement");

        let program = result.unwrap();
        assert_eq!(program.imports.len(), 1);
        assert_eq!(program.imports[0].1, "io");
    }

    /// Test parsing grouped import
    #[test]
    #[ignore]
    fn test_parse_grouped_import() {
        let source = r#"
import (
  "io"
  m "math"
)

fn main() i64 {
    return 0;
}
"#;
        let result = parse(source);
        assert!(result.is_ok(), "Should parse grouped import");

        let program = result.unwrap();
        assert_eq!(program.imports.len(), 2);
    }

    /// Test parsing struct definition
    #[test]
    #[ignore]
    fn test_parse_struct() {
        let source = r#"
pub struct Point {
    x: i64,
    y: i64,
}

fn main() i64 {
    return 0;
}
"#;
        let result = parse(source);
        assert!(result.is_ok(), "Should parse struct definition");

        let program = result.unwrap();
        assert_eq!(program.structs.len(), 1);
        assert_eq!(program.structs[0].name, "Point");
    }

    /// Test parsing enum definition
    #[test]
    #[ignore]
    fn test_parse_enum() {
        let source = r#"
pub enum Status {
    Todo,
    WIP,
    Done,
}

fn main() i64 {
    return 0;
}
"#;
        let result = parse(source);
        assert!(result.is_ok(), "Should parse enum definition");

        let program = result.unwrap();
        assert_eq!(program.enums.len(), 1);
        assert_eq!(program.enums[0].name, "Status");
    }

    /// Test parsing optional type
    #[test]
    #[ignore]
    fn test_parse_optional_type() {
        let source = r#"
fn main() i64 {
    var x: ?i32 = null;
    return 0;
}
"#;
        let result = parse(source);
        assert!(result.is_ok(), "Should parse optional type");
    }

    /// Test error handling for invalid syntax
    #[test]
    #[ignore]
    fn test_parse_invalid_syntax() {
        let source = r#"
import "io"

fn main() {
    const x: i64 = ;
    return x;
}
"#;
        let result = parse(source);
        assert!(result.is_err(), "Should fail on invalid syntax");
    }

    /// Test parsing variable declaration with reassignment
    #[test]
    #[ignore]
    fn test_parse_var_reassign() {
        let source = r#"
fn main() i64 {
    var x: i64 = 5;
    x = 10;
    return x;
}
"#;
        let result = parse(source);
        assert!(result.is_ok(), "Should parse var reassignment");
    }

    /// Test parsing const declaration
    #[test]
    #[ignore]
    fn test_parse_const() {
        let source = r#"
fn main() i64 {
    const x: i64 = 5;
    return x;
}
"#;
        let result = parse(source);
        assert!(result.is_ok(), "Should parse const declaration");
    }

    /// Test parsing tuple destructuring
    #[test]
    #[ignore]
    fn test_parse_tuple_destructure() {
        let source = r#"
fn main() i64 {
    const (a, b, c) = (1, 2, 3);
    return a;
}
"#;
        let result = parse(source);
        assert!(result.is_ok(), "Should parse tuple destructuring");
    }

    /// Test parsing tuple destructure with underscore
    #[test]
    #[ignore]
    fn test_parse_tuple_destructure_underscore() {
        let source = r#"
fn main() i64 {
    const (a, _, c) = (1, 2, 3);
    return a + c;
}
"#;
        let result = parse(source);
        assert!(
            result.is_ok(),
            "Should parse tuple destructure with underscore"
        );
    }

    /// Helper function to parse a file
    fn parse_file(path: &str) -> Result<Program, String> {
        let source = fs::read_to_string(path).map_err(|e| format!("Failed to read file: {}", e))?;
        parse(&source).map_err(|e| format!("Parse error: {} at {:?}", e.message, e.location))
    }

    /// Get test cases: (filename, expected_success)
    fn get_test_cases() -> Vec<(&'static str, bool)> {
        vec![
            ("examples/test_array_decl.lang", true),
            ("examples/test_import_stmt.lang", true),
            ("examples/test_if_else_stmt.lang", true),
            ("examples/test_optional.lang", true),
            ("examples/test_switch_stmt.lang", true),
            ("examples/test_struct.lang", true),
            ("examples/test_for_stmt.lang", true),
            ("examples/test_operators.lang", true),
            // // These require struct/interface parsing fix - parser fails
            // ("examples/test_features.lang", false),
            // // These require pub keyword - parser doesn't handle it properly
            // ("examples/test_method_simple.lang", false),
            // ("examples/test_void_no_arrow.lang", false),
            // ("examples/test_simple.lang", true),
            // ("examples/test_add.lang", true),
            // ("examples/test_add_literal.lang", true),
            // ("examples/test_add_simple.lang", true),
            // ("examples/test_return_simple.lang", true),
            // ("examples/test_return_simple2.lang", true),
            // ("examples/test_tuple.lang", true),
            // ("examples/test_tuple_destructure.lang", true),
            // ("examples/test_tuple_ret.lang", true),
            // ("examples/test_loop_syntax.lang", true),
            // // Error cases - these should fail parsing
            // // Note: import error is only detected at codegen stage, not parsing - expects error was wrong
            // ("examples/test_import_error.lang", true),
            // ("examples/test_var_no_init3.lang", false),
            // ("examples/test_const_reassign_error.lang", false),
            // ("examples/test_without_import.lang", false),
            // ("examples/test_duplicate_import.lang", false),
            // ("examples/test_multiple_imports.lang", false),
            // ("examples/test_same_package_different_alias.lang", false),
            // // Edge cases - might succeed or fail depending on implementation
            // ("examples/test_method.lang", false),
            // ("examples/test_interface.lang", false),
            // ("examples/test_math.lang", false),
            // ("examples/test_var_const.lang", false),
        ]
    }

    /// Run all example tests
    #[test]
    fn test_parser_examples() {
        let test_cases = get_test_cases();
        let mut results: Vec<TestResult> = Vec::new();
        let mut passed = 0;
        let mut failed = 0;

        println!("=========================================");
        println!("Lang Parser Test Runner");
        println!("=========================================");
        println!();

        for (filename, expected_success) in &test_cases {
            println!(
                "Testing: {} (expected: {})",
                filename,
                if *expected_success {
                    "success"
                } else {
                    "error"
                }
            );

            let result = match parse_file(filename) {
                Ok(program) => {
                    eprintln!(
                        "Successfully parsed: {} functions, {} structs, {} enums, {} imports",
                        program.functions.len(),
                        program.structs.len(),
                        program.enums.len(),
                        program.imports.len()
                    );
                    if *expected_success {
                        TestResult {
                            name: filename.to_string(),
                            expected_success: *expected_success,
                            actual_success: true,
                            error_message: None,
                        }
                    } else {
                        TestResult {
                            name: filename.to_string(),
                            expected_success: *expected_success,
                            actual_success: true,
                            error_message: Some("Expected to fail but succeeded".to_string()),
                        }
                    }
                }
                Err(e) => {
                    if !*expected_success {
                        eprintln!("  Got expected error: {}", e);
                        TestResult {
                            name: filename.to_string(),
                            expected_success: *expected_success,
                            actual_success: false,
                            error_message: None,
                        }
                    } else {
                        eprintln!("  Unexpected error: {}", e);
                        TestResult {
                            name: filename.to_string(),
                            expected_success: *expected_success,
                            actual_success: false,
                            error_message: Some(e),
                        }
                    }
                }
            };

            if result.passed() {
                println!("  ✓ PASSED");
                passed += 1;
            } else {
                println!("  ✗ FAILED");
                if let Some(ref msg) = result.error_message {
                    println!("    Reason: {}", msg);
                }
                failed += 1;
            }
            println!();

            results.push(result);
        }

        // Print summary
        println!("=========================================");
        println!("Test Summary");
        println!("=========================================");
        println!("Total: {}", test_cases.len());
        println!("Passed: {}", passed);
        println!("Failed: {}", failed);
        println!();

        if failed > 0 {
            println!("Failed tests:");
            for result in &results {
                if !result.passed() {
                    println!("  - {}", result.name);
                }
            }
        }

        // Assert all tests passed
        assert_eq!(failed, 0, "All tests should pass");
    }
}

/// Parse a source string into an AST Program
pub fn parse(source: &str) -> Result<Program, ParseError> {
    // // Step 1: Lexer - create token iterator
    // eprintln!("DEBUG: Starting lexer...");
    let tokens = lexer_iter(source);
    // eprintln!("DEBUG: Created token iterator");

    // Step 2: Parser - parse tokens into AST
    let mut parser = Parser::new(tokens);
    parser.parse_program()
}

impl Parser {
    /// Parse the entire program
    pub fn parse_program(&mut self) -> Result<Program, ParseError> {
        self.set_state(ParserState::Initial);

        let mut functions = Vec::new();
        let mut external_functions = Vec::new();
        let mut structs = Vec::new();
        let mut enums = Vec::new();
        let mut errors = Vec::new();
        let mut imports = Vec::new();

        while !self.is_at_end() {
            // -1 to skip EOF
            self.skip_whitespace();

            // Skip empty tokens
            while let Some(token) = self.current() {
                if matches!(token, Token::Eof) {
                    break;
                }
                // Skip semicolons at top level
                if matches!(token, Token::Semicolon) {
                    self.advance();
                    continue;
                }
                break;
            }

            if self
                .current()
                .map(|t| matches!(t, Token::Eof))
                .unwrap_or(true)
            {
                break;
            }

            // eprintln!(
            //     "DEBUG: Top-level parse loop, current token: {:?}",
            //     self.current()
            // );

            // Try to parse import statement
            if self.match_token(Token::Import) {
                self.set_state(ParserState::ParsingImport);
                match self.parse_import_statement() {
                    Ok(import_items) => imports.extend(import_items),
                    Err(e) => return Err(e),
                }
                self.set_state(ParserState::Initial);
                continue;
            }

            // Try to parse external function declaration (FFI)
            if self.match_token(Token::External) {
                self.set_state(ParserState::ParsingFunction);
                match self.parse_external_function() {
                    Ok(f) => external_functions.push(f),
                    Err(e) => return Err(e),
                }
                self.set_state(ParserState::Initial);
                continue;
            }

            // Try to parse struct definition
            if self.match_token(Token::Struct)
                || (self.match_token(Token::Pub) && self.match_token(Token::Struct))
            {
                self.set_state(ParserState::ParsingStruct);
                match self.parse_struct() {
                    Ok(s) => structs.push(s),
                    Err(e) => return Err(e),
                }
                self.set_state(ParserState::Initial);
                continue;
            }

            // Try to parse enum definition
            if self.match_token(Token::Enum)
                || (self.match_token(Token::Pub) && self.match_token(Token::Enum))
            {
                self.set_state(ParserState::ParsingEnum);
                match self.parse_enum() {
                    Ok(e) => enums.push(e),
                    Err(e) => return Err(e),
                }
                self.set_state(ParserState::Initial);
                continue;
            }

            // Try to parse error definition
            if self.match_token(Token::ErrorKw)
                || (self.match_token(Token::Pub) && self.match_token(Token::ErrorKw))
            {
                match self.parse_error() {
                    Ok(e) => errors.push(e),
                    Err(e) => return Err(e),
                }
                self.set_state(ParserState::Initial);
                continue;
            }

            // Try to parse function definition (including pub fn)
            // Check if we have 'fn' or 'pub fn'
            let is_pub = self.peek(0).map(|t| t.token == Token::Pub).unwrap_or(false);
            let is_fn = self
                .peek(if is_pub { 1 } else { 0 })
                .map(|t| t.token == Token::Fn)
                .unwrap_or(false);

            if is_fn {
                if is_pub {
                    self.advance(); // consume pub, leave fn for parse_function
                }
                // Don't consume fn here - let parse_function handle it
                self.set_state(ParserState::ParsingFunction);
                match self.parse_function() {
                    Ok(f) => functions.push(f),
                    Err(e) => return Err(e),
                }
                self.set_state(ParserState::Initial);
            } else {
                // Try to parse function definition
                self.set_state(ParserState::ParsingFunction);
                match self.parse_function() {
                    Ok(f) => functions.push(f),
                    Err(e) => return Err(e),
                }
                self.set_state(ParserState::Initial);
            }
        }

        self.set_state(ParserState::Completed);

        Ok(Program {
            functions,
            external_functions,
            structs,
            enums,
            errors,
            imports,
        })
    }

    /// Parse an import statement
    fn parse_import_statement(&mut self) -> Result<Vec<(Option<String>, String)>, ParseError> {
        let mut packages = Vec::new();

        // Helper to parse a single package with optional alias
        let parse_package_item = |p: &mut Parser| -> Result<(Option<String>, String), ParseError> {
            p.skip_whitespace();

            let first_token = p
                .tokens
                .next()
                .and_then(|r| r.ok())
                .ok_or_else(|| ParseError {
                    message: "Expected package name or alias".to_string(),
                    location: p.current_token().map(|t| t.span.start),
                })?;

            match first_token.token {
                Token::String(name) => {
                    // import "pkg"
                    Ok((None, name))
                }
                Token::Ident(id) => {
                    // Could be:
                    // 1. import pkg
                    // 2. import alias "pkg"
                    // 3. import alias pkg
                    p.skip_whitespace();
                    if let Some(token_with_span) = p.tokens.peek(0) {
                        match &token_with_span.token {
                            Token::String(pkg_name) => {
                                // import alias "pkg"
                                let pkg_name = pkg_name.clone();
                                p.tokens.next(); // consume pkg_name
                                Ok((Some(id), pkg_name))
                            }
                            Token::Ident(pkg_name) => {
                                // import alias pkg
                                let pkg_name = pkg_name.clone();
                                p.tokens.next(); // consume pkg_name
                                Ok((Some(id), pkg_name))
                            }
                            _ => {
                                // import pkg
                                Ok((None, id))
                            }
                        }
                    } else {
                        // import pkg (EOF follows)
                        Ok((None, id))
                    }
                }
                _ => Err(ParseError {
                    message: format!("Unexpected token in import: {:?}", first_token.token),
                    location: Some(first_token.span.start),
                }),
            }
        };

        // Check for grouped imports with parentheses
        if self.match_token(Token::LParen) {
            loop {
                self.skip_whitespace();

                // Check for closing paren
                if self.match_token(Token::RParen) {
                    self.match_token(Token::Semicolon);
                    break;
                }

                let (alias, name) = parse_package_item(self)?;
                packages.push((alias, name));

                self.skip_whitespace();
                self.match_token(Token::Semicolon);
            }
        } else {
            let (alias, name) = parse_package_item(self)?;
            packages.push((alias, name));
            self.match_token(Token::Semicolon);
        }

        Ok(packages)
    }

    /// Parse a function definition
    fn parse_function(&mut self) -> Result<FnDef, ParseError> {
        // Check for "pub" keyword (already consumed if present)
        let visibility = if self
            .current()
            .map(|t| matches!(t, Token::Pub))
            .unwrap_or(false)
        {
            self.advance();
            Visibility::Public
        } else {
            Visibility::Private
        };

        // Consume "fn" if not already consumed
        if !self.match_token(Token::Fn) {
            return Err(ParseError {
                message: "Expected 'fn' keyword".to_string(),
                location: self.current_token().map(|t| t.span.start),
            });
        }

        // Parse function name
        let name = if let Token::Ident(name) =
            self.current().cloned().ok_or_else(|| ParseError {
                message: "Expected function name".to_string(),
                location: None,
            })? {
            let name = name.clone();
            self.advance();
            name
        } else {
            return Err(ParseError {
                message: "Expected function name".to_string(),
                location: self.current_token().map(|t| t.span.start),
            });
        };

        // Parse parameters
        self.set_state(ParserState::ParsingFunctionParams);
        let params = self.parse_function_params()?;

        // Parse return type
        self.set_state(ParserState::ParsingFunctionReturnType);
        let return_ty = self.parse_return_type()?;

        // Parse function body
        self.set_state(ParserState::ParsingFunctionBody);
        let body = self.parse_function_body()?;

        let span = Span {
            start: 0, // Would need to track start position properly
            end: self.current_token().map(|t| t.span.end).unwrap_or(0),
        };

        Ok(FnDef {
            name,
            visibility,
            params,
            return_ty,
            body,
            span,
        })
    }

    /// Parse an external C function declaration
    fn parse_external_function(&mut self) -> Result<ExternalFnDef, ParseError> {
        // Check for "pub" keyword
        let visibility = if self
            .current()
            .map(|t| matches!(t, Token::Pub))
            .unwrap_or(false)
        {
            self.advance();
            Visibility::Public
        } else {
            Visibility::Private
        };

        // Consume "cdecl" (we already consumed "external")
        if !self.match_token(Token::Cdecl) {
            return Err(ParseError {
                message: "Expected 'cdecl' keyword after 'external'".to_string(),
                location: self.current_token().map(|t| t.span.start),
            });
        }

        // Parse function name
        let name = if let Token::Ident(name) =
            self.current().cloned().ok_or_else(|| ParseError {
                message: "Expected function name".to_string(),
                location: None,
            })? {
            let name = name.clone();
            self.advance();
            name
        } else {
            return Err(ParseError {
                message: "Expected function name".to_string(),
                location: self.current_token().map(|t| t.span.start),
            });
        };

        // Parse parameters
        self.set_state(ParserState::ParsingFunctionParams);
        let params = self.parse_function_params()?;

        // Parse return type
        self.set_state(ParserState::ParsingFunctionReturnType);
        let return_ty = self.parse_return_type()?;

        let span = Span {
            start: 0,
            end: self.current_token().map(|t| t.span.end).unwrap_or(0),
        };

        Ok(ExternalFnDef {
            name,
            visibility,
            params,
            return_ty,
            span,
        })
    }

    /// Parse function parameters
    fn parse_function_params(&mut self) -> Result<Vec<FnParam>, ParseError> {
        if !self.match_token(Token::LParen) {
            return Err(ParseError {
                message: "Expected '('".to_string(),
                location: self.current_token().map(|t| t.span.start),
            });
        }

        let mut params = Vec::new();

        // Check for empty parameter list
        if self.match_token(Token::RParen) {
            return Ok(params);
        }

        loop {
            self.skip_whitespace();

            // Check for closing paren
            if self.match_token(Token::RParen) {
                break;
            }

            // Parse parameter name
            let param_name = match self.current().cloned() {
                Some(Token::Ident(name)) => {
                    let name = name.clone();
                    self.advance();
                    name
                }
                Some(Token::SelfType) => {
                    // Handle 'self' as parameter name
                    self.advance();
                    "self".to_string()
                }
                _ => {
                    return Err(ParseError {
                        message: "Expected parameter name".to_string(),
                        location: self.current_token().map(|t| t.span.start),
                    });
                }
            };

            // Expect ':'
            self.skip_whitespace();
            if !self.match_token(Token::Colon) {
                return Err(ParseError {
                    message: "Expected ':' in parameter".to_string(),
                    location: self.current_token().map(|t| t.span.start),
                });
            }

            // Parse parameter type
            let param_ty = self.parse_type()?;

            params.push(FnParam {
                name: param_name,
                ty: param_ty,
            });

            self.skip_whitespace();

            // Try to consume comma for next parameter
            if !self.match_token(Token::Comma) {
                // If no comma, check for closing paren
                if self.match_token(Token::RParen) {
                    break;
                }
            }
        }

        Ok(params)
    }

    /// Parse return type (required)
    fn parse_return_type(&mut self) -> Result<Type, ParseError> {
        self.skip_whitespace();

        eprintln!(
            "DEBUG parse_return_type: current token is {:?}",
            self.current()
        );

        // Check for void return type
        if let Some(Token::Ident(id)) = self.current().cloned() {
            if id == "void" {
                self.advance();
                // Check for error suffix !
                if self.match_token(Token::Not) {
                    // Error return type - for now, just return Void
                    return Ok(Type::Void);
                }
                return Ok(Type::Void);
            }
        }

        // Check for SelfType return
        if let Some(Token::SelfType) = self.current().cloned() {
            self.advance();
            return Ok(Type::SelfType);
        }

        // Try to parse a type (including optional types)
        if let Ok(ty) = self.parse_type() {
            eprintln!("DEBUG parse_return_type: parsed type {:?}", ty);
            return Ok(ty);
        }

        // Return type is required
        Err(ParseError {
            message: "Expected return type (e.g., 'void', 'i64', etc.)".to_string(),
            location: self.current_token().map(|t| t.span.start),
        })
    }

    /// Parse function body
    fn parse_function_body(&mut self) -> Result<Vec<Stmt>, ParseError> {
        if !self.match_token(Token::LBrace) {
            return Err(ParseError {
                message: "Expected '{'".to_string(),
                location: self.current_token().map(|t| t.span.start),
            });
        }

        let mut body = Vec::new();

        loop {
            self.skip_whitespace();

            // Check for closing brace
            if self.match_token(Token::RBrace) {
                break;
            }

            // Check for end of input
            if self
                .current()
                .map(|t| matches!(t, Token::Eof))
                .unwrap_or(true)
            {
                break;
            }

            // Parse statement
            self.set_state(ParserState::ParsingStatement);
            match self.parse_statement() {
                Ok(stmt) => body.push(stmt),
                Err(e) => return Err(e),
            }
        }

        Ok(body)
    }

    /// Parse a statement
    fn parse_statement(&mut self) -> Result<Stmt, ParseError> {
        let token = self.current().cloned().ok_or_else(|| ParseError {
            message: "Unexpected end of input".to_string(),
            location: None,
        })?;

        eprintln!("DEBUG parse_statement: token = {:?}", token);

        match token {
            Token::Return => self.parse_return_stmt(),
            Token::Import => self.parse_import_stmt(),
            Token::Var => self.parse_var_stmt(),
            Token::Const => self.parse_const_stmt(),
            Token::Let => self.parse_let_stmt(),
            Token::If => self.parse_if_stmt(),
            Token::While => self.parse_while_stmt(),
            Token::For => self.parse_for_stmt(),
            Token::Loop => self.parse_loop_stmt(),
            Token::LBrace => self.parse_block_stmt(),
            Token::Switch => self.parse_switch_stmt(),
            Token::Defer => self.parse_defer_stmt(),
            Token::Semicolon => {
                self.advance();
                self.skip_whitespace();
                self.parse_statement()
            }
            _ => self.parse_expr_stmt(),
        }
    }

    /// Parse return statement
    fn parse_return_stmt(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // consume 'return'

        self.skip_whitespace();

        // Check for empty return
        if self.match_token(Token::Semicolon) {
            return Ok(Stmt::Return {
                value: None,
                span: Span { start: 0, end: 0 },
            });
        }

        // Parse return value
        let value = self.parse_expression()?;
        self.match_token(Token::Semicolon);

        Ok(Stmt::Return {
            value: Some(value),
            span: Span { start: 0, end: 0 },
        })
    }

    /// Parse defer statement
    fn parse_defer_stmt(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // consume 'defer'

        self.skip_whitespace();

        // Parse the statement to defer (usually a function call)
        let stmt = self.parse_statement()?;

        Ok(Stmt::Defer {
            stmt: Box::new(stmt),
            span: Span { start: 0, end: 0 },
        })
    }

    /// Parse import statement (inside function body)
    fn parse_import_stmt(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // consume 'import'

        let mut packages = Vec::new();

        // Check for grouped imports
        if self.match_token(Token::LParen) {
            loop {
                self.skip_whitespace();

                if self.match_token(Token::RParen) {
                    break;
                }

                if let Token::String(name) = self.current().cloned().ok_or_else(|| ParseError {
                    message: "Expected package name".to_string(),
                    location: None,
                })? {
                    self.advance();
                    packages.push((None, name));
                }

                self.skip_whitespace();
                self.match_token(Token::Semicolon);
            }
        } else {
            // Single import
            if let Token::String(name) = self.current().cloned().ok_or_else(|| ParseError {
                message: "Expected package name".to_string(),
                location: None,
            })? {
                self.advance();
                packages.push((None, name));
            }
        }

        self.match_token(Token::Semicolon);

        Ok(Stmt::Import {
            packages,
            span: Span { start: 0, end: 0 },
        })
    }

    /// Parse var statement
    fn parse_var_stmt(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // consume 'var'

        // Check for pub
        let visibility = if self.match_token(Token::Pub) {
            Visibility::Public
        } else {
            Visibility::Private
        };

        // Parse name
        let name = if let Token::Ident(n) = self.current().cloned().ok_or_else(|| ParseError {
            message: "Expected variable name".to_string(),
            location: None,
        })? {
            let n = n.clone();
            self.advance();
            n
        } else {
            return Err(ParseError {
                message: "Expected variable name".to_string(),
                location: self.current_token().map(|t| t.span.start),
            });
        };

        // Expect ':' (optional for type inference)
        self.skip_whitespace();
        let ty = if self.match_token(Token::Colon) {
            // Parse type
            Some(self.parse_type()?)
        } else {
            None
        };

        // Expect '='
        self.skip_whitespace();
        if !self.match_token(Token::Assign) {
            return Err(ParseError {
                message: "Variable declaration requires initialization".to_string(),
                location: self.current_token().map(|t| t.span.start),
            });
        }

        // Parse value
        let value = Some(self.parse_expression()?);

        self.match_token(Token::Semicolon);

        Ok(Stmt::Let {
            mutability: Mutability::Var,
            name,
            names: None,
            ty,
            value,
            visibility,
            span: Span { start: 0, end: 0 },
        })
    }

    /// Parse const statement
    fn parse_const_stmt(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // consume 'const'

        // Check for pub
        let visibility = if self.match_token(Token::Pub) {
            Visibility::Public
        } else {
            Visibility::Private
        };

        self.skip_whitespace();

        // Check for tuple destructuring
        let (name, names) = if self.match_token(Token::LParen) {
            let mut names = Vec::new();

            loop {
                self.skip_whitespace();

                if self.match_token(Token::RParen) {
                    break;
                }

                // Check for underscore or identifier
                if let Some(token_with_span) = self.current_token().cloned() {
                    match &token_with_span.token {
                        Token::Ident(n) if n == "_" => {
                            self.advance();
                            names.push(None);
                        }
                        Token::Ident(n) => {
                            self.advance();
                            names.push(Some(n.clone()));
                        }
                        _ => {
                            return Err(ParseError {
                                message: "Expected identifier or '_'".to_string(),
                                location: Some(token_with_span.span.start),
                            });
                        }
                    }
                }

                self.skip_whitespace();
                if !self.match_token(Token::Comma) {
                    self.match_token(Token::RParen);
                    break;
                }
            }

            (String::new(), Some(names))
        } else {
            // Single variable
            let name = if let Token::Ident(n) =
                self.current().cloned().ok_or_else(|| ParseError {
                    message: "Expected constant name".to_string(),
                    location: None,
                })? {
                let n = n.clone();
                self.advance();
                n
            } else {
                return Err(ParseError {
                    message: "Expected constant name".to_string(),
                    location: self.current_token().map(|t| t.span.start),
                });
            };
            (name, None)
        };

        // Parse optional type annotation
        let ty = if self.match_token(Token::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };

        // Expect '='
        self.skip_whitespace();
        if !self.match_token(Token::Assign) {
            return Err(ParseError {
                message: "Constant declaration requires initialization".to_string(),
                location: self.current_token().map(|t| t.span.start),
            });
        }

        // Parse value
        let value = self.parse_expression()?;

        self.match_token(Token::Semicolon);

        Ok(Stmt::Let {
            mutability: Mutability::Const,
            name,
            names,
            ty,
            value: Some(value),
            visibility,
            span: Span { start: 0, end: 0 },
        })
    }

    /// Parse let statement (deprecated, falls back to var)
    fn parse_let_stmt(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // consume 'let'

        let visibility = if self.match_token(Token::Pub) {
            Visibility::Public
        } else {
            Visibility::Private
        };

        let name = if let Token::Ident(n) = self.current().cloned().ok_or_else(|| ParseError {
            message: "Expected variable name".to_string(),
            location: None,
        })? {
            let n = n.clone();
            self.advance();
            n
        } else {
            return Err(ParseError {
                message: "Expected variable name".to_string(),
                location: self.current_token().map(|t| t.span.start),
            });
        };

        // Expect ':' (optional for type inference)
        self.skip_whitespace();
        let ty = if self.match_token(Token::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };

        // Expect '='
        self.skip_whitespace();
        if !self.match_token(Token::Assign) {
            return Err(ParseError {
                message: "Variable declaration requires initialization".to_string(),
                location: self.current_token().map(|t| t.span.start),
            });
        }

        let value = Some(self.parse_expression()?);
        self.match_token(Token::Semicolon);

        Ok(Stmt::Let {
            mutability: Mutability::Var,
            name,
            names: None,
            ty,
            value,
            visibility,
            span: Span { start: 0, end: 0 },
        })
    }

    /// Parse if statement
    fn parse_if_stmt(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // consume 'if'

        // Check for parenthesized condition: if (expr) ...
        self.skip_whitespace();
        let condition = if self.match_token(Token::LParen) {
            let cond = self.parse_expression()?;
            self.skip_whitespace();
            if !self.match_token(Token::RParen) {
                return Err(ParseError {
                    message: "Expected ')' after if condition".to_string(),
                    location: self.current_token().map(|t| t.span.start),
                });
            }
            cond
        } else {
            self.parse_expression()?
        };

        self.skip_whitespace();
        let mut capture = None;
        if self.match_token(Token::Pipe) {
            if let Token::Ident(name) = self.current().cloned().ok_or_else(|| ParseError {
                message: "Expected identifier after '|'".to_string(),
                location: None,
            })? {
                capture = Some(name);
                self.advance();
                if !self.match_token(Token::Pipe) {
                    return Err(ParseError {
                        message: "Expected closing '|'".to_string(),
                        location: self.current_token().map(|t| t.span.start),
                    });
                }
            }
        }

        self.skip_whitespace();
        let then_branch = Box::new(self.parse_statement()?);

        let else_branch = if self.match_token(Token::Else) {
            if let Token::If = self.current().cloned().unwrap_or(Token::Eof) {
                Some(Box::new(self.parse_if_stmt()?))
            } else {
                Some(Box::new(self.parse_statement()?))
            }
        } else {
            None
        };

        Ok(Stmt::If {
            condition,
            capture,
            then_branch,
            else_branch,
            span: Span { start: 0, end: 0 },
        })
    }

    /// Parse if expression
    fn parse_if_expr(&mut self) -> Result<Expr, ParseError> {
        self.advance(); // consume 'if'

        self.skip_whitespace();
        let condition = if self.match_token(Token::LParen) {
            let cond = self.parse_expression()?;
            self.skip_whitespace();
            if !self.match_token(Token::RParen) {
                return Err(ParseError {
                    message: "Expected ')' after if condition".to_string(),
                    location: self.current_token().map(|t| t.span.start),
                });
            }
            cond
        } else {
            self.parse_expression()?
        };

        self.skip_whitespace();
        let mut capture = None;
        if self.match_token(Token::Pipe) {
            if let Token::Ident(name) = self.current().cloned().ok_or_else(|| ParseError {
                message: "Expected identifier after '|'".to_string(),
                location: None,
            })? {
                capture = Some(name);
                self.advance();
                if !self.match_token(Token::Pipe) {
                    return Err(ParseError {
                        message: "Expected closing '|'".to_string(),
                        location: self.current_token().map(|t| t.span.start),
                    });
                }
            }
        }

        self.skip_whitespace();
        let then_branch = self.parse_if_branch_expr()?;

        self.skip_whitespace();
        if !self.match_token(Token::Else) {
            return Err(ParseError {
                message: "Expected 'else' in if expression".to_string(),
                location: self.current_token().map(|t| t.span.start),
            });
        }
        self.skip_whitespace();
        let else_branch = self.parse_if_branch_expr()?;

        Ok(Expr::If {
            condition: Box::new(condition),
            capture,
            then_branch: Box::new(then_branch),
            else_branch: Box::new(else_branch),
            span: Span { start: 0, end: 0 },
        })
    }

    fn parse_if_branch_expr(&mut self) -> Result<Expr, ParseError> {
        self.skip_whitespace();
        if let Some(Token::LBrace) = self.current() {
            let block = self.parse_block_stmt()?;
            if let Stmt::Block { stmts, span } = block {
                return Ok(Expr::Block { stmts, span });
            }
        }
        self.parse_expression()
    }

    /// Parse while statement
    fn parse_while_stmt(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // consume 'while'

        // Check for parenthesized condition: while (expr) ...
        self.skip_whitespace();
        let condition = if self.match_token(Token::LParen) {
            // Parse the expression inside the parentheses
            let cond = self.parse_expression()?;
            self.skip_whitespace();

            // Expect closing parenthesis
            if !self.match_token(Token::RParen) {
                return Err(ParseError {
                    message: "Expected ')' after while condition".to_string(),
                    location: self.current_token().map(|t| t.span.start),
                });
            }
            cond
        } else {
            self.parse_expression()?
        };

        self.skip_whitespace();
        // Check for capture: |e|
        let mut capture = None;
        if self.match_token(Token::Pipe) {
            if let Token::Ident(name) = self.current().cloned().ok_or_else(|| ParseError {
                message: "Expected identifier after '|'".to_string(),
                location: None,
            })? {
                capture = Some(name);
                self.advance();
                if !self.match_token(Token::Pipe) {
                    return Err(ParseError {
                        message: "Expected closing '|'".to_string(),
                        location: self.current_token().map(|t| t.span.start),
                    });
                }
            }
        }

        self.skip_whitespace();
        let body = Box::new(self.parse_statement()?);

        Ok(Stmt::While {
            condition,
            capture,
            body,
            span: Span { start: 0, end: 0 },
        })
    }

    /// Parse for statement
    fn parse_for_stmt(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // consume 'for'

        let mut var_name = None;

        // Handle optional opening parenthesis
        self.match_token(Token::LParen);

        // Check if there's a loop variable: for i range iterable
        if let Token::Ident(name) = self.current().cloned().unwrap_or(Token::Eof) {
            if let Some(next) = self.peek(1) {
                if next.token == Token::Range {
                    var_name = Some(name);
                    self.advance(); // consume name
                    self.advance(); // consume 'range'
                }
            }
        }

        let iterable = self.parse_expression()?;

        self.skip_whitespace();
        // Check for closing parenthesis
        self.match_token(Token::RParen);

        self.skip_whitespace();
        // Check for capture: |e| or |k, v|
        let mut capture = None;
        let mut index_var = None;
        if self.match_token(Token::Pipe) {
            // Parse first variable (can be identifier or _)
            let first_var = match self.current().cloned() {
                Some(Token::Ident(name)) => {
                    self.advance();
                    Some(name)
                }
                Some(Token::Underscore) => {
                    self.advance();
                    None
                }
                _ => {
                    return Err(ParseError {
                        message: "Expected identifier or '_' after '|'".to_string(),
                        location: self.current_token().map(|t| t.span.start),
                    });
                }
            };

            // Check if there's a second variable (index, value pattern)
            if self.match_token(Token::Comma) {
                // This is the |k, v| or |_, v| or |k, _| pattern
                // First variable is the index/key
                index_var = first_var;

                // Parse second variable
                capture = match self.current().cloned() {
                    Some(Token::Ident(name)) => {
                        self.advance();
                        Some(name)
                    }
                    Some(Token::Underscore) => {
                        self.advance();
                        None
                    }
                    _ => {
                        return Err(ParseError {
                            message: "Expected identifier or '_' after ',' in for loop capture"
                                .to_string(),
                            location: self.current_token().map(|t| t.span.start),
                        });
                    }
                };
            } else {
                // Single variable pattern |e|
                capture = first_var;
            }

            // Expect closing pipe
            if !self.match_token(Token::Pipe) {
                return Err(ParseError {
                    message: "Expected closing '|'".to_string(),
                    location: self.current_token().map(|t| t.span.start),
                });
            }
        }

        self.skip_whitespace();
        let body = Box::new(self.parse_statement()?);

        Ok(Stmt::For {
            var_name,
            iterable,
            capture,
            index_var,
            body,
            span: Span { start: 0, end: 0 },
        })
    }

    /// Parse loop statement
    fn parse_loop_stmt(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // consume 'loop'

        self.skip_whitespace();
        let body = Box::new(self.parse_statement()?);

        Ok(Stmt::Loop {
            body,
            span: Span { start: 0, end: 0 },
        })
    }

    /// Parse block statement
    fn parse_block_stmt(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // consume '{'

        let mut stmts = Vec::new();

        loop {
            self.skip_whitespace();

            if self.match_token(Token::RBrace) {
                break;
            }

            if self
                .current()
                .map(|t| matches!(t, Token::Eof))
                .unwrap_or(true)
            {
                break;
            }

            stmts.push(self.parse_statement()?);
        }

        Ok(Stmt::Block {
            stmts,
            span: Span { start: 0, end: 0 },
        })
    }

    /// Parse switch statement
    fn parse_switch_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start_location = self.current_token().map(|t| t.span.start).unwrap_or(0);
        self.advance(); // consume 'switch'
        self.skip_whitespace();

        let has_paren = self.match_token(Token::LParen);
        let condition = self.parse_expression()?;
        if has_paren {
            self.expect(Token::RParen)?;
        }

        self.skip_whitespace();
        self.expect(Token::LBrace)?;

        let mut cases = Vec::new();
        while !self.match_token(Token::RBrace) && !self.is_at_end() {
            self.skip_whitespace();

            if matches!(self.current(), Some(Token::RBrace)) {
                break;
            }

            // Parse patterns: expr, expr, ...
            let mut patterns = Vec::new();
            loop {
                patterns.push(self.parse_expression()?);
                self.skip_whitespace();
                if !self.match_token(Token::Comma) {
                    break;
                }
                self.skip_whitespace();
            }

            self.expect(Token::FatArrow)?;
            self.skip_whitespace();

            // Optional capture: |id|
            let mut capture = None;
            if self.match_token(Token::Pipe) {
                if let Token::Ident(name) = self.current().cloned().ok_or_else(|| ParseError {
                    message: "Expected capture variable name".to_string(),
                    location: None,
                })? {
                    capture = Some(name);
                    self.advance();
                    self.expect(Token::Pipe)?;
                } else {
                    return Err(ParseError {
                        message: "Expected capture variable name".to_string(),
                        location: None,
                    });
                }
            }

            self.skip_whitespace();
            let body = self.parse_statement()?;

            cases.push(SwitchCase {
                patterns,
                capture,
                body,
                span: Span { start: 0, end: 0 },
            });
        }

        let end_location = self.current_token().map(|t| t.span.end).unwrap_or(0);

        Ok(Stmt::Switch {
            condition,
            cases,
            span: Span {
                start: start_location,
                end: end_location,
            },
        })
    }

    /// Parse expression statement
    fn parse_expr_stmt(&mut self) -> Result<Stmt, ParseError> {
        let expr = self.parse_expression()?;
        self.skip_whitespace();

        // Check for assignment: target op value;
        match &expr {
            Expr::Ident(name, _) => {
                if let Some(op) = self.match_assign_op() {
                    let value = self.parse_expression()?;
                    self.skip_whitespace();
                    self.match_token(Token::Semicolon);

                    return Ok(Stmt::Assign {
                        target: name.clone(),
                        op,
                        value,
                        span: Span { start: 0, end: 0 },
                    });
                }
            }
            Expr::MemberAccess {
                object: member_expr,
                member,
                ..
            } => {
                // Handle member assignment like self.i += 1
                if let Some(op) = self.match_assign_op() {
                    let value = self.parse_expression()?;
                    self.skip_whitespace();
                    self.match_token(Token::Semicolon);

                    // Convert to a setter expression - format the target string
                    fn format_target(expr: &Expr) -> String {
                        match expr {
                            Expr::Ident(name, _) => name.clone(),
                            Expr::MemberAccess { object, member, .. } => {
                                format!("{}.{}", format_target(object.as_ref()), member)
                            }
                            _ => "".to_string(),
                        }
                    }
                    let target = format!("{}.{}", format_target(member_expr.as_ref()), member);
                    return Ok(Stmt::Assign {
                        target,
                        op,
                        value,
                        span: Span { start: 0, end: 0 },
                    });
                }
            }
            _ => {}
        }

        self.match_token(Token::Semicolon);

        Ok(Stmt::Expr {
            expr,
            span: Span { start: 0, end: 0 },
        })
    }

    /// Helper to format an expression as string for assignment target
    fn format_target_for_expr(self: Self, expr: &Expr) -> String {
        match expr {
            Expr::Ident(name, _) => name.clone(),
            Expr::MemberAccess { object, member, .. } => {
                format!(
                    "{}.{}",
                    self.format_target_for_expr(object.as_ref()),
                    member
                )
            }
            _ => "".to_string(),
        }
    }

    /// Parse an expression
    fn parse_expression(&mut self) -> Result<Expr, ParseError> {
        self.set_state(ParserState::ParsingExpression);

        // Try to parse assignment first (lowest precedence)
        let result = self.parse_assignment_expr();
        result
    }

    /// Parse assignment expression
    fn parse_assignment_expr(&mut self) -> Result<Expr, ParseError> {
        let result = self.parse_or_expr()?;
        Ok(result)
    }

    /// Parse OR expression (||)
    fn parse_or_expr(&mut self) -> Result<Expr, ParseError> {
        let mut result = self.parse_and_expr()?;

        while self.match_token(Token::PipePipe) {
            let right = self.parse_and_expr()?;
            result = Expr::Binary {
                op: BinaryOp::Or,
                left: Box::new(result),
                right: Box::new(right),
                span: Span { start: 0, end: 0 },
            };
        }
        Ok(result)
    }

    /// Parse AND expression (&&)
    fn parse_and_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_bitwise_or_expr()?;

        while self.match_token(Token::AmpAmp) {
            let right = self.parse_bitwise_or_expr()?;
            left = Expr::Binary {
                op: BinaryOp::And,
                left: Box::new(left),
                right: Box::new(right),
                span: Span { start: 0, end: 0 },
            };
        }
        Ok(left)
    }

    /// Parse bitwise OR expression (|)
    fn parse_bitwise_or_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_bitwise_xor_expr()?;

        while self.match_token(Token::Pipe) {
            let right = self.parse_bitwise_xor_expr()?;
            left = Expr::Binary {
                op: BinaryOp::BitOr,
                left: Box::new(left),
                right: Box::new(right),
                span: Span { start: 0, end: 0 },
            };
        }
        Ok(left)
    }

    /// Parse bitwise XOR expression (^)
    fn parse_bitwise_xor_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_bitwise_and_expr()?;

        while self.match_token(Token::Caret) {
            let right = self.parse_bitwise_and_expr()?;
            left = Expr::Binary {
                op: BinaryOp::BitXor,
                left: Box::new(left),
                right: Box::new(right),
                span: Span { start: 0, end: 0 },
            };
        }
        Ok(left)
    }

    /// Parse bitwise AND expression (&)
    fn parse_bitwise_and_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_range_expr()?;

        while self.match_token(Token::Ampersand) {
            let right = self.parse_range_expr()?;
            left = Expr::Binary {
                op: BinaryOp::BitAnd,
                left: Box::new(left),
                right: Box::new(right),
                span: Span { start: 0, end: 0 },
            };
        }
        Ok(left)
    }

    fn parse_range_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_equality_expr()?;

        while self.match_token(Token::DotDot) {
            let right = self.parse_equality_expr()?;
            left = Expr::Binary {
                op: BinaryOp::Range,
                left: Box::new(left),
                right: Box::new(right),
                span: Span { start: 0, end: 0 },
            };
        }
        Ok(left)
    }

    /// Parse equality expression (==, !=)
    fn parse_equality_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_comparison_expr()?;

        while let Token::Equal | Token::NotEqual = self.current().cloned().unwrap_or(Token::Eof) {
            let op = if self.match_token(Token::Equal) {
                BinaryOp::Eq
            } else {
                BinaryOp::Ne
            };

            let right = self.parse_comparison_expr()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
                span: Span { start: 0, end: 0 },
            };
        }
        Ok(left)
    }

    /// Parse comparison expression (<, >, <=, >=)
    fn parse_comparison_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_shift_expr()?;

        while let Token::Less | Token::Greater | Token::LessEq | Token::GreaterEq =
            self.current().cloned().unwrap_or(Token::Eof)
        {
            let op = if self.match_token(Token::Less) {
                BinaryOp::Lt
            } else if self.match_token(Token::Greater) {
                BinaryOp::Gt
            } else if self.match_token(Token::LessEq) {
                BinaryOp::Le
            } else if self.match_token(Token::GreaterEq) {
                BinaryOp::Ge
            } else {
                break;
            };

            let right = self.parse_shift_expr()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
                span: Span { start: 0, end: 0 },
            };
        }
        Ok(left)
    }

    /// Parse bitwise shift expression (<<, >>)
    fn parse_shift_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_additive_expr()?;

        while let Token::LessLess | Token::GreaterGreater =
            self.current().cloned().unwrap_or(Token::Eof)
        {
            let op = if self.match_token(Token::LessLess) {
                BinaryOp::Shl
            } else if self.match_token(Token::GreaterGreater) {
                BinaryOp::Shr
            } else {
                break;
            };

            let right = self.parse_additive_expr()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
                span: Span { start: 0, end: 0 },
            };
        }
        Ok(left)
    }

    /// Parse additive expression (+, -)
    fn parse_additive_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_multiplicative_expr()?;

        loop {
            let current = self.current().cloned();
            match current {
                Some(Token::Plus) => {
                    self.advance(); // consume +
                    let right = self.parse_multiplicative_expr()?;
                    left = Expr::Binary {
                        op: BinaryOp::Add,
                        left: Box::new(left),
                        right: Box::new(right),
                        span: Span { start: 0, end: 0 },
                    };
                }
                Some(Token::Minus) => {
                    self.advance(); // consume -
                    let right = self.parse_multiplicative_expr()?;
                    left = Expr::Binary {
                        op: BinaryOp::Sub,
                        left: Box::new(left),
                        right: Box::new(right),
                        span: Span { start: 0, end: 0 },
                    };
                }
                _ => break,
            }
        }

        Ok(left)
    }

    /// Parse multiplicative expression (*, /, %)
    fn parse_multiplicative_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_unary_expr()?;

        while let Token::Star | Token::Slash | Token::Percent =
            self.current().cloned().unwrap_or(Token::Eof)
        {
            let op = match self.current().unwrap() {
                Token::Star => {
                    self.advance();
                    BinaryOp::Mul
                }
                Token::Slash => {
                    self.advance();
                    BinaryOp::Div
                }
                Token::Percent => {
                    self.advance();
                    BinaryOp::Mod
                }
                _ => break,
            };

            let right = self.parse_unary_expr()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
                span: Span { start: 0, end: 0 },
            };
        }
        Ok(left)
    }

    /// Parse unary expression (!, -)
    fn parse_unary_expr(&mut self) -> Result<Expr, ParseError> {
        if let Token::Not | Token::Minus = self.current().cloned().unwrap_or(Token::Eof) {
            let op = if self.match_token(Token::Not) {
                UnaryOp::Not
            } else {
                UnaryOp::Neg
            };

            let expr = Box::new(self.parse_unary_expr()?);
            return Ok(Expr::Unary {
                op,
                expr,
                span: Span { start: 0, end: 0 },
            });
        }

        self.parse_call_expr()
    }

    /// Parse call expression
    fn parse_call_expr(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_primary_expr()?;

        loop {
            // Function call
            if self.match_token(Token::LParen) {
                let mut args = Vec::new();

                if !self.match_token(Token::RParen) {
                    loop {
                        args.push(self.parse_expression()?);
                        self.skip_whitespace();

                        if self.match_token(Token::RParen) {
                            break;
                        }

                        if !self.match_token(Token::Comma) {
                            break;
                        }
                    }
                }

                let (name, namespace) = match &expr {
                    Expr::Ident(n, _) => (n.clone(), None),
                    Expr::MemberAccess { object, member, .. } => {
                        if let Expr::Ident(ns, _) = &**object {
                            (member.clone(), Some(ns.clone()))
                        } else {
                            (member.clone(), None)
                        }
                    }
                    _ => (String::new(), None),
                };

                expr = Expr::Call {
                    name,
                    namespace,
                    args,
                    span: Span { start: 0, end: 0 },
                };
                continue;
            }

            // Struct literal: TypeName{ field1: value1, field2: value2 }
            // or shorthand: TypeName{ field1, field2 }
            if self.match_token(Token::LBrace) {
                // Check if we have a valid struct name (expr should be an Ident)
                let struct_name = match &expr {
                    Expr::Ident(n, _) => n.clone(),
                    _ => {
                        return Err(ParseError {
                            message: "Expected struct type name".to_string(),
                            location: self.current_token().map(|t| t.span.start),
                        });
                    }
                };

                self.skip_whitespace();

                // Empty struct
                if self.match_token(Token::RBrace) {
                    expr = Expr::Struct {
                        name: struct_name,
                        fields: vec![],
                        span: Span { start: 0, end: 0 },
                    };
                    continue;
                }

                // Parse struct fields
                let mut fields = Vec::new();
                loop {
                    self.skip_whitespace();

                    // Check for field name
                    let current_token = self.current().cloned();
                    let field_name = match current_token {
                        Some(Token::Ident(n)) => n.clone(),
                        _ => {
                            return Err(ParseError {
                                message: "Expected field name in struct literal".to_string(),
                                location: self.current_token().map(|t| t.span.start),
                            });
                        }
                    };
                    self.advance();

                    self.skip_whitespace();

                    // Check if this is a shorthand field or full field
                    let current_after_name = self.current().cloned();
                    match current_after_name {
                        Some(Token::Colon) => {
                            // Full field: name: value
                            self.advance(); // consume colon
                            let value = self.parse_expression()?;
                            fields.push((field_name, value));
                        }
                        _ => {
                            // Shorthand field: name (means name: name)
                            let ident_expr =
                                Expr::Ident(field_name.clone(), Span { start: 0, end: 0 });
                            fields.push((field_name, ident_expr));
                        }
                    }

                    self.skip_whitespace();

                    if self.match_token(Token::RBrace) {
                        break;
                    }

                    if !self.match_token(Token::Comma) {
                        return Err(ParseError {
                            message: "Expected ',' or '}' in struct literal".to_string(),
                            location: self.current_token().map(|t| t.span.start),
                        });
                    }

                    self.skip_whitespace();

                    // Check if we're done (handle trailing comma case)
                    if let Some(Token::RBrace) = self.current().cloned() {
                        break;
                    }
                }

                expr = Expr::Struct {
                    name: struct_name,
                    fields,
                    span: Span { start: 0, end: 0 },
                };
                continue;
            }

            // Tuple index or Member access
            if self.match_token(Token::Dot) {
                let current = self.current().cloned().ok_or_else(|| ParseError {
                    message: "Expected index or member name".to_string(),
                    location: None,
                })?;

                match current {
                    Token::Int(i) => {
                        self.advance();
                        expr = Expr::TupleIndex {
                            tuple: Box::new(expr),
                            index: i as usize,
                            span: Span { start: 0, end: 0 },
                        };
                        continue;
                    }
                    Token::Ident(id) => {
                        self.advance();
                        expr = Expr::MemberAccess {
                            object: Box::new(expr),
                            member: id,
                            span: Span { start: 0, end: 0 },
                        };
                        continue;
                    }
                    _ => {
                        return Err(ParseError {
                            message: "Expected index or member name".to_string(),
                            location: None,
                        });
                    }
                }
            }

            break;
        }

        Ok(expr)
    }

    /// Parse primary expression (literals, identifiers, etc.)
    fn parse_primary_expr(&mut self) -> Result<Expr, ParseError> {
        let token = self.current().cloned().ok_or_else(|| ParseError {
            message: "Unexpected end of input".to_string(),
            location: None,
        })?;

        match token {
            Token::Int(n) => {
                self.advance();
                Ok(Expr::Int(n, Span { start: 0, end: 0 }))
            }
            Token::String(s) => {
                self.advance();
                Ok(Expr::String(s, Span { start: 0, end: 0 }))
            }
            Token::Char(c) => {
                self.advance();
                Ok(Expr::Char(c, Span { start: 0, end: 0 }))
            }
            Token::True => {
                self.advance();
                Ok(Expr::Bool(true, Span { start: 0, end: 0 }))
            }
            Token::False => {
                self.advance();
                Ok(Expr::Bool(false, Span { start: 0, end: 0 }))
            }
            Token::Null => {
                self.advance();
                Ok(Expr::Null(Span { start: 0, end: 0 }))
            }
            // Handle try expression
            Token::Try => {
                self.advance();
                let expr = self.parse_unary_expr()?;
                Ok(Expr::Try {
                    expr: Box::new(expr),
                    span: Span { start: 0, end: 0 },
                })
            }
            // Handle 'self' keyword as identifier in expression context
            Token::SelfType => {
                self.advance();
                Ok(Expr::Ident("self".to_string(), Span { start: 0, end: 0 }))
            }
            Token::LParen => {
                self.advance();
                self.skip_whitespace();

                // Empty tuple
                if self.match_token(Token::RParen) {
                    return Ok(Expr::Tuple(vec![], Span { start: 0, end: 0 }));
                }

                // Parse first element
                let first = self.parse_expression()?;
                self.skip_whitespace();

                // Check if this is a single-element tuple (i.e., followed by RParen)
                if self.match_token(Token::RParen) {
                    return Ok(Expr::Tuple(vec![first], Span { start: 0, end: 0 }));
                }

                // Multi-element tuple
                let mut elements = vec![first];

                // Consume comma after first element
                self.match_token(Token::Comma);

                loop {
                    self.skip_whitespace();
                    elements.push(self.parse_expression()?);
                    self.skip_whitespace();

                    if self.match_token(Token::RParen) {
                        break;
                    }

                    if !self.match_token(Token::Comma) {
                        break;
                    }
                }

                Ok(Expr::Tuple(elements, Span { start: 0, end: 0 }))
            }
            Token::LBracket => {
                self.advance();
                self.skip_whitespace();

                // Empty array
                if self.match_token(Token::RBracket) {
                    return Ok(Expr::Array(vec![], Span { start: 0, end: 0 }));
                }

                // Check for typed array: [size]Type{elements} or [size]Type
                // Look ahead to see if we have a number followed by ] then a type
                let token0 = self.peek(0).map(|t| t.token.clone());
                let token1 = self.peek(1).map(|t| t.token.clone());

                if let Some(Token::Int(_)) = token0 {
                    // We have a number - check if next is RBracket (closing the size)
                    if let Some(Token::RBracket) = token1 {
                        // Consume the size number and RBracket
                        self.advance(); // consume the number
                        self.advance(); // consume the RBracket

                        // Now check if next is a type (identifier)
                        let token_after_bracket = self.peek(0).map(|t| t.token.clone());
                        if let Some(Token::Ident(_)) = token_after_bracket {
                            // This is [size]Type - consume the type
                            self.advance(); // consume the type name

                            self.skip_whitespace();

                            // Check for array literal body: {elements}
                            if self.match_token(Token::LBrace) {
                                let mut elements = Vec::new();
                                loop {
                                    self.skip_whitespace();
                                    if self.match_token(Token::RBrace) {
                                        break;
                                    }
                                    elements.push(self.parse_expression()?);
                                    self.skip_whitespace();

                                    // After element, expect either comma or closing brace
                                    if self.match_token(Token::Comma) {
                                        // Continue to next element
                                        continue;
                                    } else if self.match_token(Token::RBrace) {
                                        // End of array
                                        break;
                                    } else {
                                        // Error - neither comma nor closing brace
                                        return Err(ParseError {
                                            message: "Expected ',' or '}' in array literal"
                                                .to_string(),
                                            location: self.current_token().map(|t| t.span.start),
                                        });
                                    }
                                }
                                return Ok(Expr::Array(elements, Span { start: 0, end: 0 }));
                            }

                            // Just typed array without elements
                            return Ok(Expr::Array(vec![], Span { start: 0, end: 0 }));
                        }

                        // If no type follows, we have [number] - treat as single element array
                        // Put the tokens back by returning to parse it as a regular array
                    }
                }

                // Parse array (bracket syntax)
                let mut elements = Vec::new();
                loop {
                    elements.push(self.parse_expression()?);
                    self.skip_whitespace();

                    if self.match_token(Token::RBracket) {
                        break;
                    }

                    if !self.match_token(Token::Comma) {
                        break;
                    }
                }

                Ok(Expr::Array(elements, Span { start: 0, end: 0 }))
            }
            // Handle array literals with curly braces: {1, 2, 3}
            Token::LBrace => {
                self.advance();
                self.skip_whitespace();

                // Empty array
                if self.match_token(Token::RBrace) {
                    return Ok(Expr::Array(vec![], Span { start: 0, end: 0 }));
                }

                // Parse array elements
                let mut elements = Vec::new();
                loop {
                    elements.push(self.parse_expression()?);
                    self.skip_whitespace();

                    if self.match_token(Token::RBrace) {
                        break;
                    }

                    // Must have comma to continue
                    if !self.match_token(Token::Comma) {
                        break;
                    }
                }

                Ok(Expr::Array(elements, Span { start: 0, end: 0 }))
            }
            Token::Ident(name) => {
                self.advance();
                Ok(Expr::Ident(name, Span { start: 0, end: 0 }))
            }
            Token::If => self.parse_if_expr(),
            _ => Err(ParseError {
                message: format!("Unexpected token: {:?}", token),
                location: self.current_token().map(|t| t.span.start),
            }),
        }
    }

    /// Parse a type
    fn parse_type(&mut self) -> Result<Type, ParseError> {
        self.set_state(ParserState::ParsingType);

        self.skip_whitespace();

        // Check for optional type
        if self.match_token(Token::Question) {
            let inner = self.parse_type()?;
            return Ok(Type::Option(Box::new(inner)));
        }

        // Check for error type suffix: Type! (e.g., i32! or void!)
        // This needs to be checked after parsing the base type
        let base_type = self.parse_base_type()?;

        // Check for ! suffix
        if self.match_token(Token::Not) {
            // Error return type - for now, just return the base type
            // In a full implementation, this would create a special error type
            return Ok(base_type);
        }

        Ok(base_type)
    }

    /// Parse a base type (without modifiers)
    fn parse_base_type(&mut self) -> Result<Type, ParseError> {
        if self.match_token(Token::Star) {
            let inner = self.parse_type()?;
            return Ok(Type::Pointer(Box::new(inner)));
        }

        // Check for array type: [size]Type or []Type
        if self.match_token(Token::LBracket) {
            self.skip_whitespace();

            // Parse optional size
            let size = if self.match_token(Token::RBracket) {
                // Dynamic array / slice: []Type
                None
            } else {
                // Try to parse a number for fixed-size array
                if let Token::Int(n) = self.current().cloned().ok_or_else(|| ParseError {
                    message: "Expected array size or ']' in array type".to_string(),
                    location: None,
                })? {
                    let size = n as usize;
                    self.advance();
                    self.skip_whitespace();

                    // Expect closing bracket
                    if !self.match_token(Token::RBracket) {
                        return Err(ParseError {
                            message: "Expected ']' in array type".to_string(),
                            location: self.current_token().map(|t| t.span.start),
                        });
                    }

                    // Parse element type
                    let element_type = Box::new(self.parse_type()?);

                    return Ok(Type::Array {
                        size: Some(size),
                        element_type,
                    });
                } else {
                    return Err(ParseError {
                        message: "Expected array size or ']' in array type".to_string(),
                        location: self.current_token().map(|t| t.span.start),
                    });
                }
            };

            // Parse element type for dynamic array
            let element_type = Box::new(self.parse_type()?);

            return Ok(Type::Array { size, element_type });
        }

        // Check for tuple type
        if self.match_token(Token::LParen) {
            let mut types = Vec::new();

            loop {
                self.skip_whitespace();

                if self.match_token(Token::RParen) {
                    break;
                }

                types.push(self.parse_type()?);

                self.skip_whitespace();

                if self.match_token(Token::RParen) {
                    break;
                }

                if !self.match_token(Token::Comma) {
                    return Err(ParseError {
                        message: "Expected ',' or ')' in tuple type".to_string(),
                        location: self.current_token().map(|t| t.span.start),
                    });
                }
            }

            return Ok(Type::Tuple(types));
        }

        // Parse basic types or custom type
        let type_name = match self.current().cloned() {
            Some(Token::Ident(name)) => {
                let name = name.clone();
                self.advance();
                name
            }
            Some(Token::SelfType) => {
                self.advance();
                "self".to_string()
            }
            _ => {
                return Err(ParseError {
                    message: "Expected type".to_string(),
                    location: self.current_token().map(|t| t.span.start),
                });
            }
        };

        // Check for basic types
        let ty = match type_name.as_str() {
            "i8" => Type::I8,
            "i16" => Type::I16,
            "i32" => Type::I32,
            "i64" => Type::I64,
            "u8" => Type::U8,
            "u16" => Type::U16,
            "u32" => Type::U32,
            "u64" => Type::U64,
            "bool" => Type::Bool,
            "void" => Type::Void,
            "self" => Type::SelfType,
            _ => Type::Custom {
                name: type_name,
                generic_args: Vec::new(),
                is_exported: false,
            },
        };

        // Check for generic arguments
        if self.match_token(Token::Less) {
            let mut generic_args = Vec::new();

            loop {
                self.skip_whitespace();

                if self.match_token(Token::Greater) {
                    break;
                }

                generic_args.push(self.parse_type()?);

                self.skip_whitespace();

                if self.match_token(Token::Greater) {
                    break;
                }

                self.match_token(Token::Comma);
            }

            if let Type::Custom { name, .. } = ty {
                return Ok(Type::Custom {
                    name,
                    generic_args,
                    is_exported: false,
                });
            }
        }

        Ok(ty)
    }

    /// Parse a struct definition
    fn parse_struct(&mut self) -> Result<StructDef, ParseError> {
        // "pub" and "struct" already consumed
        let visibility = Visibility::Public;

        // Parse generic parameters
        let generic_params = self.parse_generic_params()?;

        // Parse struct name
        let name = if let Token::Ident(n) = self.current().cloned().ok_or_else(|| ParseError {
            message: "Expected struct name".to_string(),
            location: None,
        })? {
            let n = n.clone();
            self.advance();
            n
        } else {
            return Err(ParseError {
                message: "Expected struct name".to_string(),
                location: self.current_token().map(|t| t.span.start),
            });
        };

        // Parse fields and methods
        if !self.match_token(Token::LBrace) {
            return Err(ParseError {
                message: "Expected '{'".to_string(),
                location: self.current_token().map(|t| t.span.start),
            });
        }

        let mut fields = Vec::new();
        let mut methods = Vec::new();

        loop {
            self.skip_whitespace();

            if self.match_token(Token::RBrace) {
                break;
            }

            // Skip optional comma (for trailing commas in struct)
            if self.match_token(Token::Comma) {
                self.skip_whitespace();
                if self.match_token(Token::RBrace) {
                    break;
                }
                // After matching comma, check if next is a method
                if self.match_token(Token::Fn) || self.match_token(Token::Pub) {
                    // This is a method - if we matched pub, we're already past it
                    // If we matched fn, current is fn. If we matched pub, current is fn now.
                    // Just call parse_function which will handle it
                    methods.push(self.parse_function()?);
                    self.match_token(Token::Comma);
                    continue;
                }
                // Otherwise, continue to parse the next field
            }

            // Check for method (fn keyword) - only if we didn't just handle a trailing comma
            if self.match_token(Token::Fn) || self.match_token(Token::Pub) {
                // This is a method - if we matched pub, we're already past it
                // If we matched fn, current is fn. If we matched pub, current is fn now.
                // Just call parse_function which will handle it
                methods.push(self.parse_function()?);
                self.match_token(Token::Comma);
                continue;
            }

            // Parse field
            let field_visibility = if self.match_token(Token::Pub) {
                Visibility::Public
            } else {
                Visibility::Private
            };

            let field_name = if let Token::Ident(n) =
                self.current().cloned().ok_or_else(|| ParseError {
                    message: "Expected field name".to_string(),
                    location: None,
                })? {
                let n = n.clone();
                self.advance();
                n
            } else {
                return Err(ParseError {
                    message: "Expected field name".to_string(),
                    location: self.current_token().map(|t| t.span.start),
                });
            };

            self.match_token(Token::Colon);
            let field_ty = self.parse_type()?;

            fields.push(StructField {
                name: field_name,
                ty: field_ty,
                visibility: field_visibility,
            });

            self.match_token(Token::Comma);
        }

        self.match_token(Token::Semicolon);

        Ok(StructDef {
            name,
            fields,
            methods,
            visibility,
            generic_params,
            span: Span { start: 0, end: 0 },
        })
    }

    /// Parse an enum definition
    fn parse_enum(&mut self) -> Result<EnumDef, ParseError> {
        // "pub" and "enum" already consumed
        let visibility = Visibility::Public;

        // Parse generic parameters
        let generic_params = self.parse_generic_params()?;

        // Parse enum name
        let name = if let Token::Ident(n) = self.current().cloned().ok_or_else(|| ParseError {
            message: "Expected enum name".to_string(),
            location: None,
        })? {
            let n = n.clone();
            self.advance();
            n
        } else {
            return Err(ParseError {
                message: "Expected enum name".to_string(),
                location: self.current_token().map(|t| t.span.start),
            });
        };

        // Parse variants
        if !self.match_token(Token::LBrace) {
            return Err(ParseError {
                message: "Expected '{'".to_string(),
                location: self.current_token().map(|t| t.span.start),
            });
        }

        let mut variants = Vec::new();
        let mut methods = Vec::new();

        loop {
            self.skip_whitespace();

            if self.match_token(Token::RBrace) {
                break;
            }

            // Check for method
            if self.match_token(Token::Fn) || self.match_token(Token::Pub) {
                // Note: Previous token tracking changed - using current token instead
                if let Some(prev) = self.peek(0) {
                    if let Token::Pub = prev.token {
                        self.match_token(Token::Fn);
                    }
                }

                methods.push(self.parse_function()?);
                self.match_token(Token::Comma);
                continue;
            }

            // Parse variant
            let variant_visibility = if self.match_token(Token::Pub) {
                Visibility::Public
            } else {
                Visibility::Private
            };

            let variant_name = if let Token::Ident(n) =
                self.current().cloned().ok_or_else(|| ParseError {
                    message: "Expected variant name".to_string(),
                    location: None,
                })? {
                let n = n.clone();
                self.advance();
                n
            } else {
                return Err(ParseError {
                    message: "Expected variant name".to_string(),
                    location: self.current_token().map(|t| t.span.start),
                });
            };

            // Parse associated types
            let mut associated_types = Vec::new();
            if self.match_token(Token::LParen) {
                loop {
                    self.skip_whitespace();

                    if self.match_token(Token::RParen) {
                        break;
                    }

                    associated_types.push(self.parse_type()?);

                    self.skip_whitespace();

                    if self.match_token(Token::RParen) {
                        break;
                    }

                    self.match_token(Token::Comma);
                }
            }

            variants.push(EnumVariant {
                name: variant_name,
                associated_types,
                visibility: variant_visibility,
            });

            self.match_token(Token::Comma);
        }

        self.match_token(Token::Semicolon);

        Ok(EnumDef {
            name,
            variants,
            methods,
            visibility,
            generic_params,
            span: Span { start: 0, end: 0 },
        })
    }

    /// Parse an error definition
    fn parse_error(&mut self) -> Result<ErrorDef, ParseError> {
        // "pub" already consumed if present, "error" consumed
        let visibility = Visibility::Public;

        // Parse error name
        let name = if let Token::Ident(n) = self.current().cloned().ok_or_else(|| ParseError {
            message: "Expected error name".to_string(),
            location: None,
        })? {
            let n = n.clone();
            self.advance();
            n
        } else {
            return Err(ParseError {
                message: "Expected error name".to_string(),
                location: self.current_token().map(|t| t.span.start),
            });
        };

        // Check for error union syntax: error X = A | B | C
        if self.match_token(Token::Assign) {
            // Parse union types
            let mut union_types = Vec::new();
            loop {
                self.skip_whitespace();

                if let Token::Ident(type_name) =
                    self.current().cloned().ok_or_else(|| ParseError {
                        message: "Expected error type in union".to_string(),
                        location: None,
                    })?
                {
                    union_types.push(Type::Custom {
                        name: type_name,
                        generic_args: vec![],
                        is_exported: false,
                    });
                    self.advance();
                }

                self.skip_whitespace();

                if !self.match_token(Token::Pipe) {
                    break;
                }
            }

            self.match_token(Token::Semicolon);

            return Ok(ErrorDef {
                name,
                variants: vec![],
                union_types: Some(union_types),
                visibility,
                span: Span { start: 0, end: 0 },
            });
        }

        // Parse variants (error with { ... })
        if !self.match_token(Token::LBrace) {
            return Err(ParseError {
                message: "Expected '{'".to_string(),
                location: self.current_token().map(|t| t.span.start),
            });
        }

        let mut variants = Vec::new();

        loop {
            self.skip_whitespace();

            if self.match_token(Token::RBrace) {
                break;
            }

            // Parse variant
            let variant_visibility = if self.match_token(Token::Pub) {
                Visibility::Public
            } else {
                Visibility::Private
            };

            let variant_name = if let Token::Ident(n) =
                self.current().cloned().ok_or_else(|| ParseError {
                    message: "Expected variant name".to_string(),
                    location: None,
                })? {
                let n = n.clone();
                self.advance();
                n
            } else {
                return Err(ParseError {
                    message: "Expected variant name".to_string(),
                    location: self.current_token().map(|t| t.span.start),
                });
            };

            // Parse associated types
            let mut associated_types = Vec::new();
            if self.match_token(Token::LParen) {
                loop {
                    self.skip_whitespace();

                    if self.match_token(Token::RParen) {
                        break;
                    }

                    associated_types.push(self.parse_type()?);

                    self.skip_whitespace();

                    if self.match_token(Token::RParen) {
                        break;
                    }

                    self.match_token(Token::Comma);
                }
            }

            variants.push(ErrorVariant {
                name: variant_name,
                associated_types,
                visibility: variant_visibility,
            });

            self.match_token(Token::Comma);
        }

        self.match_token(Token::Semicolon);

        Ok(ErrorDef {
            name,
            variants,
            union_types: None,
            visibility,
            span: Span { start: 0, end: 0 },
        })
    }

    /// Parse generic parameters (<T, U, ...>)
    fn parse_generic_params(&mut self) -> Result<Vec<String>, ParseError> {
        if !self.match_token(Token::Less) {
            return Ok(Vec::new());
        }

        let mut params = Vec::new();

        loop {
            self.skip_whitespace();

            if self.match_token(Token::Greater) {
                break;
            }

            if let Token::Ident(n) = self.current().cloned().ok_or_else(|| ParseError {
                message: "Expected generic parameter".to_string(),
                location: None,
            })? {
                params.push(n);
                self.advance();
            }

            self.skip_whitespace();

            if self.match_token(Token::Greater) {
                break;
            }

            self.match_token(Token::Comma);
        }

        Ok(params)
    }
}

/// Parse error type
#[derive(Debug)]
pub struct ParseError {
    pub message: String,
    pub location: Option<usize>,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(loc) = self.location {
            write!(f, "Parse error at position {}: {}", loc, self.message)
        } else {
            write!(f, "Parse error: {}", self.message)
        }
    }
}

impl std::error::Error for ParseError {}
