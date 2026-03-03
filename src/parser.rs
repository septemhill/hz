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
        eprintln!(
            "DEBUG: Parser state transition: {:?} -> {:?}",
            self.state.name(),
            new_state.name()
        );
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
        self.tokens.peek()
    }

    /// Get current token value (for internal use with mutable reference)
    fn current(&mut self) -> Option<&Token> {
        self.tokens.peek().map(|t| &t.token)
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
        if let Some(current) = self.tokens.peek() {
            if std::mem::discriminant(&current.token) == std::mem::discriminant(&token) {
                self.advance();
                return true;
            }
        }
        false
    }

    /// Peek at current token without consuming
    fn peek(&mut self) -> Option<&TokenWithSpan> {
        self.tokens.peek()
    }

    /// Peek at nth token ahead (0 = current, 1 = next)
    fn peek_n(&mut self, n: usize) -> Option<&TokenWithSpan> {
        self.tokens.peek_n(n)
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

/// Parse a source string into an AST Program
pub fn parse(source: &str) -> Result<Program, ParseError> {
    // Step 1: Lexer - create token iterator
    eprintln!("DEBUG: Starting lexer...");
    let tokens = lexer_iter(source);
    eprintln!("DEBUG: Created token iterator");

    // Step 2: Parser - parse tokens into AST
    let mut parser = Parser::new(tokens);
    parser.parse_program()
}

impl Parser {
    /// Parse the entire program
    pub fn parse_program(&mut self) -> Result<Program, ParseError> {
        self.set_state(ParserState::Initial);

        let mut functions = Vec::new();
        let mut structs = Vec::new();
        let mut enums = Vec::new();
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

            eprintln!(
                "DEBUG: Top-level parse loop, current token: {:?}",
                self.current()
            );

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

            // Try to parse function definition
            self.set_state(ParserState::ParsingFunction);
            match self.parse_function() {
                Ok(f) => functions.push(f),
                Err(e) => return Err(e),
            }
            self.set_state(ParserState::Initial);
        }

        self.set_state(ParserState::Completed);

        Ok(Program {
            functions,
            structs,
            enums,
            imports,
        })
    }

    /// Parse an import statement
    fn parse_import_statement(&mut self) -> Result<Vec<(Option<String>, String)>, ParseError> {
        let mut packages = Vec::new();

        // Check for grouped imports with parentheses
        if self.match_token(Token::LParen) {
            loop {
                self.skip_whitespace();

                // Check for closing paren
                if self.match_token(Token::RParen) {
                    self.match_token(Token::Semicolon);
                    break;
                }

                // Check for alias first (identifier followed by string)
                let alias: Option<String> = if let Some(Token::Ident(id)) = self.current().cloned()
                {
                    if let Some(Token::String(_)) = self.peek_n(1).map(|t| &t.token) {
                        Some(id.clone())
                    } else {
                        None
                    }
                } else {
                    None
                };

                if alias.is_some() {
                    self.advance(); // consume alias
                }

                // Parse string literal for package name
                if let Token::String(name) = self.current().cloned().ok_or_else(|| ParseError {
                    message: "Expected package name".to_string(),
                    location: None,
                })? {
                    self.advance();
                    packages.push((alias, name));
                } else {
                    return Err(ParseError {
                        message: "Expected package name in quotes".to_string(),
                        location: self.current_token().map(|t| t.span.start),
                    });
                }

                self.skip_whitespace();
                self.match_token(Token::Semicolon);
            }
        } else {
            // Single import: "package" or alias "package"
            self.skip_whitespace();

            // Check for alias (identifier followed by string)
            let alias: Option<String> = if let Some(Token::Ident(id)) = self.current().cloned() {
                if let Some(Token::String(_)) = self.peek_n(1).map(|t| &t.token) {
                    Some(id.clone())
                } else {
                    None
                }
            } else {
                None
            };

            if alias.is_some() {
                self.advance(); // consume alias
            }

            // Parse package name
            if let Token::String(name) = self.current().cloned().ok_or_else(|| ParseError {
                message: "Expected package name".to_string(),
                location: None,
            })? {
                self.advance();
                packages.push((alias, name));
            } else {
                return Err(ParseError {
                    message: "Expected package name in quotes".to_string(),
                    location: self.current_token().map(|t| t.span.start),
                });
            }

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
            let param_name = if let Token::Ident(name) =
                self.current().cloned().ok_or_else(|| ParseError {
                    message: "Expected parameter name".to_string(),
                    location: None,
                })? {
                let name = name.clone();
                self.advance();
                name
            } else {
                return Err(ParseError {
                    message: "Expected parameter name".to_string(),
                    location: self.current_token().map(|t| t.span.start),
                });
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

    /// Parse return type
    fn parse_return_type(&mut self) -> Result<Option<Type>, ParseError> {
        self.skip_whitespace();

        // Check for void return type
        if let Some(Token::Ident(id)) = self.current().cloned() {
            if id == "void" {
                self.advance();
                return Ok(Some(Type::Void));
            }
        }

        // Try to parse a type
        if let Ok(ty) = self.parse_type() {
            return Ok(Some(ty));
        }

        Ok(None)
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

        match token {
            Token::Return => self.parse_return_stmt(),
            Token::Import => self.parse_import_stmt(),
            Token::Var => self.parse_var_stmt(),
            Token::Const => self.parse_const_stmt(),
            Token::Let => self.parse_let_stmt(),
            Token::If => self.parse_if_stmt(),
            Token::While => self.parse_while_stmt(),
            Token::Loop => self.parse_loop_stmt(),
            Token::LBrace => self.parse_block_stmt(),
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

        // Expect ':'
        self.skip_whitespace();
        self.match_token(Token::Colon);

        // Parse type
        let ty = Some(self.parse_type()?);

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

                // Check for underscore
                if self.match_token(Token::Not) {
                    // Using Not for _ since we don't have underscore token
                    names.push(None);
                } else if let Token::Ident(n) =
                    self.current().cloned().ok_or_else(|| ParseError {
                        message: "Expected identifier".to_string(),
                        location: None,
                    })?
                {
                    self.advance();
                    names.push(Some(n));
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

        self.match_token(Token::Semicolon);

        Ok(Stmt::Let {
            mutability: Mutability::Var,
            name,
            names: None,
            ty: None,
            value: None,
            visibility,
            span: Span { start: 0, end: 0 },
        })
    }

    /// Parse if statement
    fn parse_if_stmt(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // consume 'if'

        let condition = self.parse_expression()?;
        let then_branch = Box::new(self.parse_statement()?);

        let else_branch = if self.match_token(Token::Else) {
            Some(Box::new(self.parse_statement()?))
        } else {
            None
        };

        Ok(Stmt::If {
            condition,
            then_branch,
            else_branch,
            span: Span { start: 0, end: 0 },
        })
    }

    /// Parse while statement
    fn parse_while_stmt(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // consume 'while'

        let condition = self.parse_expression()?;
        let body = Box::new(self.parse_statement()?);

        Ok(Stmt::While {
            condition,
            body,
            span: Span { start: 0, end: 0 },
        })
    }

    /// Parse loop statement
    fn parse_loop_stmt(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // consume 'loop'

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

    /// Parse expression statement
    fn parse_expr_stmt(&mut self) -> Result<Stmt, ParseError> {
        let expr = self.parse_expression()?;
        self.match_token(Token::Semicolon);

        Ok(Stmt::Expr {
            expr,
            span: Span { start: 0, end: 0 },
        })
    }

    /// Parse an expression
    fn parse_expression(&mut self) -> Result<Expr, ParseError> {
        self.set_state(ParserState::ParsingExpression);

        // Try to parse assignment first (lowest precedence)
        self.parse_assignment_expr()
    }

    /// Parse assignment expression
    fn parse_assignment_expr(&mut self) -> Result<Expr, ParseError> {
        let left = self.parse_or_expr()?;
        Ok(left)
    }

    /// Parse OR expression (||)
    fn parse_or_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_and_expr()?;

        while self.match_token(Token::Pipe) {
            let right = self.parse_and_expr()?;
            left = Expr::Binary {
                op: BinaryOp::Or,
                left: Box::new(left),
                right: Box::new(right),
                span: Span { start: 0, end: 0 },
            };
        }

        Ok(left)
    }

    /// Parse AND expression (&&)
    fn parse_and_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_equality_expr()?;

        while self.match_token(Token::Ampersand) {
            let right = self.parse_equality_expr()?;
            left = Expr::Binary {
                op: BinaryOp::And,
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
        let mut left = self.parse_additive_expr()?;

        while let Token::Less | Token::Greater | Token::LessEq | Token::GreaterEq =
            self.current().cloned().unwrap_or(Token::Eof)
        {
            let op = match self.current().unwrap() {
                Token::Less => {
                    self.advance();
                    BinaryOp::Lt
                }
                Token::Greater => {
                    self.advance();
                    BinaryOp::Gt
                }
                Token::LessEq => {
                    self.advance();
                    BinaryOp::Le
                }
                Token::GreaterEq => {
                    self.advance();
                    BinaryOp::Ge
                }
                _ => break,
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

        while let Token::Plus | Token::Minus = self.current().cloned().unwrap_or(Token::Eof) {
            let op = if self.match_token(Token::Plus) {
                BinaryOp::Add
            } else {
                BinaryOp::Sub
            };

            let right = self.parse_multiplicative_expr()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
                span: Span { start: 0, end: 0 },
            };
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

                expr = Expr::Call {
                    name: match &expr {
                        Expr::Ident(n, _) => n.clone(),
                        _ => {
                            return Err(ParseError {
                                message: "Expected function name".to_string(),
                                location: None,
                            });
                        }
                    },
                    namespace: None,
                    args,
                    span: Span { start: 0, end: 0 },
                };
                continue;
            }

            // Tuple index access
            if self.match_token(Token::Dot) {
                if let Token::Int(i) = self.current().cloned().ok_or_else(|| ParseError {
                    message: "Expected index".to_string(),
                    location: None,
                })? {
                    self.advance();
                    expr = Expr::TupleIndex {
                        tuple: Box::new(expr),
                        index: i as usize,
                        span: Span { start: 0, end: 0 },
                    };
                    continue;
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
            Token::LParen => {
                self.advance();
                self.skip_whitespace();

                // Empty tuple
                if self.match_token(Token::RParen) {
                    return Ok(Expr::Tuple(vec![], Span { start: 0, end: 0 }));
                }

                // Parse tuple
                let mut elements = Vec::new();
                loop {
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
            Token::Ident(name) => {
                self.advance();
                Ok(Expr::Ident(name, Span { start: 0, end: 0 }))
            }
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

        // Parse basic types or custom type
        let type_name = if let Token::Ident(name) =
            self.current().cloned().ok_or_else(|| ParseError {
                message: "Expected type".to_string(),
                location: None,
            })? {
            let name = name.clone();
            self.advance();
            name
        } else {
            return Err(ParseError {
                message: "Expected type".to_string(),
                location: self.current_token().map(|t| t.span.start),
            });
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

            // Check for method (fn keyword)
            if self.match_token(Token::Fn) || self.match_token(Token::Pub) {
                // This is a method, go back
                // Note: Previous token tracking changed - using current token instead
                if let Some(prev) = self.peek() {
                    if let Token::Pub = prev.token {
                        // Already consumed pub, now consume fn
                        self.match_token(Token::Fn);
                    }
                }

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
                if let Some(prev) = self.peek() {
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
