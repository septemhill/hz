//! # Parser for Lang Programming Language
//!
//! This module parses source code and generates AST nodes.

use crate::ast::*;

/// Parse a source string into an AST Program
pub fn parse(source: &str) -> Result<Program, ParseError> {
    let mut chars: Vec<char> = source.chars().collect();
    let mut pos = 0;

    let mut functions = Vec::new();

    while pos < chars.len() {
        skip_whitespace(&chars, &mut pos);
        if pos >= chars.len() {
            break;
        }

        if try_parse_fn(&chars, &mut pos)
            .map(|f| functions.push(f))
            .is_err()
        {
            return Err(ParseError {
                message: format!("Unexpected token at position {}", pos),
                location: Some(pos),
            });
        }

        skip_whitespace(&chars, &mut pos);
    }

    Ok(Program { functions })
}

/// Try to parse a function definition
fn try_parse_fn(chars: &[char], pos: &mut usize) -> Result<FnDef, ParseError> {
    // Expect "fn"
    if !try_consume_keyword(chars, pos, "fn") {
        return Err(ParseError {
            message: "Expected 'fn' keyword".to_string(),
            location: Some(*pos),
        });
    }

    skip_whitespace(chars, pos);

    // Parse function name
    let name = parse_ident(chars, pos)?;

    // Expect "("
    skip_whitespace(chars, pos);
    if !try_consume(chars, pos, '(') {
        return Err(ParseError {
            message: "Expected '('".to_string(),
            location: Some(*pos),
        });
    }

    // Parse parameters (simplified - no parameters for now)
    skip_whitespace(chars, pos);
    try_consume(chars, pos, ')');

    // Expect "{"
    skip_whitespace(chars, pos);
    if !try_consume(chars, pos, '{') {
        return Err(ParseError {
            message: "Expected '{{'".to_string(),
            location: Some(*pos),
        });
    }

    // Parse function body (simplified)
    let mut body = Vec::new();
    let start = *pos;

    while *pos < chars.len() {
        skip_whitespace(chars, pos);
        if *pos >= chars.len() {
            break;
        }

        if try_consume(chars, pos, '}') {
            break;
        }

        // Try to parse a statement
        if let Ok(stmt) = try_parse_stmt(chars, pos) {
            body.push(stmt);
        } else {
            (*pos) += 1;
        }
    }

    let span = span(start, *pos);

    Ok(FnDef {
        name,
        params: Vec::new(),
        return_ty: None,
        body,
        span,
    })
}

/// Try to parse a statement
fn try_parse_stmt(chars: &[char], pos: &mut usize) -> Result<Stmt, ParseError> {
    let start = *pos;
    skip_whitespace(chars, pos);

    // Try to parse "return"
    if try_consume_keyword(chars, pos, "return") {
        skip_whitespace(chars, pos);

        if try_consume(chars, pos, ';') {
            return Ok(Stmt::Return {
                value: None,
                span: span(start, *pos),
            });
        }

        // Parse expression
        let expr = try_parse_expr(chars, pos)?;
        try_consume(chars, pos, ';');
        return Ok(Stmt::Return {
            value: Some(expr),
            span: span(start, *pos),
        });
    }

    // Try to parse "let"
    if try_consume_keyword(chars, pos, "let") {
        skip_whitespace(chars, pos);
        let name = parse_ident(chars, pos)?;

        skip_whitespace(chars, pos);
        try_consume(chars, pos, ';');

        return Ok(Stmt::Let {
            mutable: false,
            name,
            ty: None,
            value: None,
            span: span(start, *pos),
        });
    }

    // Try to parse expression statement
    let expr = try_parse_expr(chars, pos)?;
    try_consume(chars, pos, ';');

    Ok(Stmt::Expr {
        expr,
        span: span(start, *pos),
    })
}

/// Try to parse an expression
fn try_parse_expr(chars: &[char], pos: &mut usize) -> Result<Expr, ParseError> {
    let start = *pos;
    skip_whitespace(chars, pos);

    // Try to parse integer literal
    if chars[*pos].is_ascii_digit() {
        let mut num_str = String::new();
        while *pos < chars.len() && chars[*pos].is_ascii_digit() {
            num_str.push(chars[*pos]);
            (*pos) += 1;
        }

        let value: i64 = num_str.parse().map_err(|_| ParseError {
            message: "Invalid number".to_string(),
            location: Some(*pos),
        })?;

        return Ok(Expr::Int(value, span(start, *pos)));
    }

    // Try to parse identifier
    if chars[*pos].is_alphabetic() || chars[*pos] == '_' {
        let name = parse_ident(chars, pos)?;

        // Check if it's a function call
        skip_whitespace(chars, pos);
        if try_consume(chars, pos, '(') {
            // Parse function call
            let mut args = Vec::new();
            skip_whitespace(chars, pos);

            if !try_consume(chars, pos, ')') {
                loop {
                    args.push(try_parse_expr(chars, pos)?);
                    skip_whitespace(chars, pos);

                    if try_consume(chars, pos, ')') {
                        break;
                    }

                    if !try_consume(chars, pos, ',') {
                        break;
                    }
                }
            }

            return Ok(Expr::Call {
                name,
                args,
                span: span(start, *pos),
            });
        }

        return Ok(Expr::Ident(name, span(start, *pos)));
    }

    // Try to parse binary expression
    let left = try_parse_expr(chars, pos)?;
    skip_whitespace(chars, pos);

    // Look for binary operator
    let op = if *pos + 1 < chars.len() {
        let op_str: String = chars[*pos..*pos + 2].iter().collect();
        match op_str.as_str() {
            "+=" => {
                (*pos) += 2;
                Some(BinaryOp::Add)
            }
            "-=" => {
                (*pos) += 2;
                Some(BinaryOp::Sub)
            }
            "*=" => {
                (*pos) += 2;
                Some(BinaryOp::Mul)
            }
            "/=" => {
                (*pos) += 2;
                Some(BinaryOp::Div)
            }
            "==" => {
                (*pos) += 2;
                Some(BinaryOp::Eq)
            }
            "!=" => {
                (*pos) += 2;
                Some(BinaryOp::Ne)
            }
            "<=" => {
                (*pos) += 2;
                Some(BinaryOp::Le)
            }
            ">=" => {
                (*pos) += 2;
                Some(BinaryOp::Ge)
            }
            "&&" => {
                (*pos) += 2;
                Some(BinaryOp::And)
            }
            "||" => {
                (*pos) += 2;
                Some(BinaryOp::Or)
            }
            _ => {
                let c = chars[*pos];
                *pos += 1;
                match c {
                    '+' => Some(BinaryOp::Add),
                    '-' => Some(BinaryOp::Sub),
                    '*' => Some(BinaryOp::Mul),
                    '/' => Some(BinaryOp::Div),
                    '%' => Some(BinaryOp::Mod),
                    '<' => Some(BinaryOp::Lt),
                    '>' => Some(BinaryOp::Gt),
                    _ => None,
                }
            }
        }
    } else {
        None
    };

    if let Some(op) = op {
        let right = try_parse_expr(chars, pos)?;
        return Ok(Expr::Binary {
            op,
            left: Box::new(left),
            right: Box::new(right),
            span: span(start, *pos),
        });
    }

    // Try unary operators
    if chars[*pos] == '!' {
        (*pos) += 1;
        let expr = try_parse_expr(chars, pos)?;
        return Ok(Expr::Unary {
            op: UnaryOp::Not,
            expr: Box::new(expr),
            span: span(start, *pos),
        });
    }

    if chars[*pos] == '-' {
        (*pos) += 1;
        let expr = try_parse_expr(chars, pos)?;
        return Ok(Expr::Unary {
            op: UnaryOp::Neg,
            expr: Box::new(expr),
            span: span(start, *pos),
        });
    }

    Err(ParseError {
        message: "Expected expression".to_string(),
        location: Some(*pos),
    })
}

/// Parse an identifier
fn parse_ident(chars: &[char], pos: &mut usize) -> Result<String, ParseError> {
    let start = *pos;

    while *pos < chars.len() && (chars[*pos].is_alphanumeric() || chars[*pos] == '_') {
        (*pos) += 1;
    }

    if start == *pos {
        return Err(ParseError {
            message: "Expected identifier".to_string(),
            location: Some(*pos),
        });
    }

    Ok(chars[start..*pos].iter().collect())
}

/// Try to consume a specific character
fn try_consume(chars: &[char], pos: &mut usize, c: char) -> bool {
    skip_whitespace(chars, pos);
    if *pos < chars.len() && chars[*pos] == c {
        (*pos) += 1;
        return true;
    }
    false
}

/// Try to consume a keyword
fn try_consume_keyword(chars: &[char], pos: &mut usize, keyword: &str) -> bool {
    let start = *pos;
    skip_whitespace(chars, pos);

    let keyword_chars: Vec<char> = keyword.chars().collect();

    if *pos + keyword_chars.len() <= chars.len() {
        let slice = &chars[*pos..*pos + keyword_chars.len()];
        if slice.iter().collect::<String>() == keyword {
            // Make sure it's a complete word
            if *pos + keyword_chars.len() >= chars.len()
                || !chars[*pos + keyword_chars.len()].is_alphanumeric()
            {
                *pos += keyword_chars.len();
                return true;
            }
        }
    }

    *pos = start;
    false
}

/// Skip whitespace
fn skip_whitespace(chars: &[char], pos: &mut usize) {
    while *pos < chars.len() && chars[*pos].is_whitespace() {
        (*pos) += 1;
    }
}

/// Create span
fn span(start: usize, end: usize) -> Span {
    Span { start, end }
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
