//! # Parser for Lang Programming Language
//!
//! This module parses source code and generates AST nodes.

use crate::ast::*;

/// Parse a source string into an AST Program
pub fn parse(source: &str) -> Result<Program, ParseError> {
    let mut chars: Vec<char> = source.chars().collect();
    let mut pos = 0;

    let mut functions = Vec::new();
    let mut structs = Vec::new();
    let mut enums = Vec::new();

    while pos < chars.len() {
        skip_whitespace(&chars, &mut pos);
        if pos >= chars.len() {
            break;
        }

        // Try to parse struct definition
        if try_parse_struct(&chars, &mut pos)
            .map(|s| structs.push(s))
            .is_ok()
        {
            skip_whitespace(&chars, &mut pos);
            continue;
        }

        // Try to parse enum definition
        if try_parse_enum(&chars, &mut pos)
            .map(|e| enums.push(e))
            .is_ok()
        {
            skip_whitespace(&chars, &mut pos);
            continue;
        }

        // Try to parse function definition
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

    Ok(Program {
        functions,
        structs,
        enums,
    })
}

/// Try to parse a function definition
fn try_parse_fn(chars: &[char], pos: &mut usize) -> Result<FnDef, ParseError> {
    // Check for "pub" keyword (visibility modifier)
    let visibility = if try_consume_keyword(chars, pos, "pub") {
        Visibility::Public
    } else {
        Visibility::Private
    };

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

    // Parse parameters
    let mut params = Vec::new();
    skip_whitespace(chars, pos);
    while !try_consume(chars, pos, ')') {
        if *pos >= chars.len() {
            break;
        }

        // Parse parameter name
        let param_name = parse_ident(chars, pos)?;

        // Expect ':'
        skip_whitespace(chars, pos);
        if !try_consume(chars, pos, ':') {
            return Err(ParseError {
                message: "Expected ':' in parameter".to_string(),
                location: Some(*pos),
            });
        }

        // Parse parameter type
        let param_ty = parse_type(chars, pos)?;

        params.push(FnParam {
            name: param_name,
            ty: param_ty,
        });

        // Try to consume comma for next parameter
        skip_whitespace(chars, pos);
        if !try_consume(chars, pos, ',') {
            // If no comma, try to find closing paren
            skip_whitespace(chars, pos);
            if try_consume(chars, pos, ')') {
                break;
            }
        }
    }

    // Parse optional return type (only basic types supported for now)
    let mut return_ty = None;
    skip_whitespace(chars, pos);
    if *pos < chars.len() && chars[*pos] == '-' {
        (*pos) += 1;
        if *pos < chars.len() && chars[*pos] == '>' {
            (*pos) += 1;
            // Parse the return type
            skip_whitespace(chars, pos);
            let remaining: String = chars[*pos..].iter().take(10).collect();
            if remaining.starts_with("i8") {
                *pos += 2;
                return_ty = Some(Type::I8);
            } else if remaining.starts_with("i16") {
                *pos += 3;
                return_ty = Some(Type::I16);
            } else if remaining.starts_with("i32") {
                *pos += 3;
                return_ty = Some(Type::I32);
            } else if remaining.starts_with("i64") {
                *pos += 3;
                return_ty = Some(Type::I64);
            } else if remaining.starts_with("u8") {
                *pos += 2;
                return_ty = Some(Type::U8);
            } else if remaining.starts_with("u16") {
                *pos += 3;
                return_ty = Some(Type::U16);
            } else if remaining.starts_with("u32") {
                *pos += 3;
                return_ty = Some(Type::U32);
            } else if remaining.starts_with("u64") {
                *pos += 3;
                return_ty = Some(Type::U64);
            } else if remaining.starts_with("bool") {
                *pos += 4;
                return_ty = Some(Type::Bool);
            } else if remaining.starts_with("void") {
                *pos += 4;
                return_ty = Some(Type::Void);
            } else {
                // Not a basic type, rewind
                *pos -= 2;
            }
        } else {
            *pos -= 1;
        }
    }

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
        visibility,
        params,
        return_ty,
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

    // Try to parse "var"
    if try_consume_keyword(chars, pos, "var") {
        // Check for "pub" keyword
        let visibility = if try_consume_keyword(chars, pos, "pub") {
            Visibility::Public
        } else {
            Visibility::Private
        };

        skip_whitespace(chars, pos);
        let name = parse_ident(chars, pos)?;

        // Parse optional type annotation
        let ty = if try_consume(chars, pos, ':') {
            Some(parse_type(chars, pos)?)
        } else {
            None
        };

        // Parse optional initializer
        let value = if try_consume(chars, pos, '=') {
            Some(try_parse_expr(chars, pos)?)
        } else {
            None
        };

        try_consume(chars, pos, ';');

        return Ok(Stmt::Let {
            mutability: Mutability::Var,
            name,
            ty,
            value,
            visibility,
            span: span(start, *pos),
        });
    }

    // Try to parse "const"
    if try_consume_keyword(chars, pos, "const") {
        // Check for "pub" keyword
        let visibility = if try_consume_keyword(chars, pos, "pub") {
            Visibility::Public
        } else {
            Visibility::Private
        };

        skip_whitespace(chars, pos);
        let name = parse_ident(chars, pos)?;

        // Parse optional type annotation
        let ty = if try_consume(chars, pos, ':') {
            Some(parse_type(chars, pos)?)
        } else {
            None
        };

        // Const requires initializer
        try_consume(chars, pos, '=');
        let value = try_parse_expr(chars, pos)?;

        try_consume(chars, pos, ';');

        return Ok(Stmt::Let {
            mutability: Mutability::Const,
            name,
            ty,
            value: Some(value),
            visibility,
            span: span(start, *pos),
        });
    }

    // Try to parse assignment statement (identifier followed by assignment operator)
    if chars[*pos].is_alphabetic() || chars[*pos] == '_' {
        let old_pos = *pos;
        let name = parse_ident(chars, pos)?;
        skip_whitespace(chars, pos);

        // Check for assignment operators
        let assign_op = if *pos < chars.len() {
            if try_consume(chars, pos, '=') {
                // Check if it's = (simple assignment) and not == (comparison)
                if *pos < chars.len() && chars[*pos] != '=' {
                    Some(AssignOp::Assign)
                } else {
                    *pos = old_pos; // Reset position, it's a comparison
                    None
                }
            } else if try_consume(chars, pos, '+') && try_consume(chars, pos, '=') {
                Some(AssignOp::AddAssign)
            } else if try_consume(chars, pos, '-') && try_consume(chars, pos, '=') {
                Some(AssignOp::SubAssign)
            } else if try_consume(chars, pos, '*') && try_consume(chars, pos, '=') {
                Some(AssignOp::MulAssign)
            } else if try_consume(chars, pos, '/') && try_consume(chars, pos, '=') {
                Some(AssignOp::DivAssign)
            } else {
                None
            }
        } else {
            None
        };

        if let Some(op) = assign_op {
            let value = try_parse_expr(chars, pos)?;
            try_consume(chars, pos, ';');
            return Ok(Stmt::Assign {
                target: name,
                op,
                value,
                span: span(start, *pos),
            });
        }

        // Not an assignment, reset position
        *pos = old_pos;
    }

    // Try to parse "let" (deprecated, fall back to var)
    if try_consume_keyword(chars, pos, "let") {
        // Check for "pub" keyword
        let visibility = if try_consume_keyword(chars, pos, "pub") {
            Visibility::Public
        } else {
            Visibility::Private
        };

        skip_whitespace(chars, pos);
        let name = parse_ident(chars, pos)?;

        skip_whitespace(chars, pos);
        try_consume(chars, pos, ';');

        return Ok(Stmt::Let {
            mutability: Mutability::Var,
            name,
            ty: None,
            value: None,
            visibility,
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

    // Try to parse string literal
    if *pos < chars.len() && chars[*pos] == '"' {
        (*pos) += 1; // consume opening quote
        let mut value = String::new();
        while *pos < chars.len() && chars[*pos] != '"' {
            value.push(chars[*pos]);
            (*pos) += 1;
        }
        if *pos < chars.len() && chars[*pos] == '"' {
            (*pos) += 1; // consume closing quote
        }
        return Ok(Expr::String(value, span(start, *pos)));
    }

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

        // Check for namespaced call (e.g., io.println)
        skip_whitespace(chars, pos);
        let namespace = if *pos < chars.len() && chars[*pos] == '.' {
            (*pos) += 1; // consume '.'
            let ns = parse_ident(chars, pos)?;
            Some(ns)
        } else {
            None
        };

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

            // Swap namespace and name for namespaced calls (io.println -> name=println, namespace=io)
            let (final_name, final_namespace) = if let Some(ns) = namespace {
                (ns, Some(name))
            } else {
                (name, None)
            };

            return Ok(Expr::Call {
                name: final_name,
                namespace: final_namespace,
                args,
                span: span(start, *pos),
            });
        }

        // If not a function call but has namespace, it's an error (for now)
        if namespace.is_some() {
            return Err(ParseError {
                message: "Unexpected namespace without function call".to_string(),
                location: Some(*pos),
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

/// Try to parse a struct definition
/// Syntax: pub? struct<T>? Name { field: Type, ... }
fn try_parse_struct(chars: &[char], pos: &mut usize) -> Result<StructDef, ParseError> {
    let start = *pos;

    // Check for "pub" keyword
    let visibility = if try_consume_keyword(chars, pos, "pub") {
        Visibility::Public
    } else {
        Visibility::Private
    };

    // Expect "struct"
    if !try_consume_keyword(chars, pos, "struct") {
        return Err(ParseError {
            message: "Expected 'struct' keyword".to_string(),
            location: Some(*pos),
        });
    }

    skip_whitespace(chars, pos);

    // Try to parse generic parameters BEFORE the name (e.g., struct<T>)
    let mut generic_params = Vec::new();
    if *pos < chars.len() && chars[*pos] == '<' {
        (*pos) += 1; // consume '<'
        loop {
            skip_whitespace(chars, pos);
            if *pos >= chars.len() {
                break;
            }
            if chars[*pos] == '>' {
                (*pos) += 1; // consume '>'
                break;
            }
            if chars[*pos] == ',' {
                (*pos) += 1; // consume ','
                continue;
            }
            let param = parse_ident(chars, pos)?;
            generic_params.push(param);
            skip_whitespace(chars, pos);
            if *pos >= chars.len() {
                break;
            }
            if chars[*pos] == '>' {
                (*pos) += 1; // consume '>'
                break;
            }
        }
    }

    skip_whitespace(chars, pos);

    // Parse struct name
    let name = parse_ident(chars, pos)?;

    // If no generic params before name, try after name (e.g., struct Name<T>)
    if generic_params.is_empty() {
        skip_whitespace(chars, pos);
        if *pos < chars.len() && chars[*pos] == '<' {
            (*pos) += 1; // consume '<'
            loop {
                skip_whitespace(chars, pos);
                if *pos >= chars.len() {
                    break;
                }
                if chars[*pos] == '>' {
                    (*pos) += 1; // consume '>'
                    break;
                }
                if chars[*pos] == ',' {
                    (*pos) += 1; // consume ','
                    continue;
                }
                let param = parse_ident(chars, pos)?;
                generic_params.push(param);
                skip_whitespace(chars, pos);
                if *pos >= chars.len() {
                    break;
                }
                if chars[*pos] == '>' {
                    (*pos) += 1; // consume '>'
                    break;
                }
            }
        }
    }

    // Expect "{"
    skip_whitespace(chars, pos);
    if !try_consume(chars, pos, '{') {
        return Err(ParseError {
            message: "Expected '{{'".to_string(),
            location: Some(*pos),
        });
    }

    // Parse fields and methods
    let mut fields = Vec::new();
    let mut methods = Vec::new();
    loop {
        skip_whitespace(chars, pos);
        if *pos >= chars.len() {
            break;
        }
        if try_consume(chars, pos, '}') {
            break;
        }

        // Check if it's a function definition (method) - look for "pub fn" or just "fn"
        let method_start = *pos;
        let is_fn = {
            // Try to find "fn" keyword after optional "pub"
            let mut test_pos = *pos;
            skip_whitespace(chars, &mut test_pos);
            if try_consume_keyword(chars, &mut test_pos, "pub") {
                skip_whitespace(chars, &mut test_pos);
                try_consume_keyword(chars, &mut test_pos, "fn")
            } else {
                try_consume_keyword(chars, &mut test_pos, "fn")
            }
        };

        if is_fn {
            // Parse as method
            *pos = method_start;
            match try_parse_fn(chars, pos) {
                Ok(method) => {
                    methods.push(method);
                    skip_whitespace(chars, pos);
                    try_consume(chars, pos, ',');
                    continue;
                }
                Err(e) => {
                    // Failed to parse as method, try as field
                    // Reset position
                }
            }
        }

        // Parse as field
        let field_visibility = if try_consume_keyword(chars, pos, "pub") {
            Visibility::Public
        } else {
            Visibility::Private
        };

        // Parse field name
        skip_whitespace(chars, pos);
        let field_name = parse_ident(chars, pos)?;

        // Expect ":"
        skip_whitespace(chars, pos);
        if !try_consume(chars, pos, ':') {
            return Err(ParseError {
                message: "Expected ':' in struct field".to_string(),
                location: Some(*pos),
            });
        }

        // Parse field type
        skip_whitespace(chars, pos);
        let field_ty = parse_type(chars, pos)?;

        fields.push(StructField {
            name: field_name,
            ty: field_ty,
            visibility: field_visibility,
        });

        skip_whitespace(chars, pos);
        try_consume(chars, pos, ',');
    }

    // Consume optional semicolon after struct
    skip_whitespace(chars, pos);
    try_consume(chars, pos, ';');

    let span = span(start, *pos);

    Ok(StructDef {
        name,
        fields,
        methods,
        visibility,
        generic_params,
        span,
    })
}

/// Try to parse an enum definition
/// Syntax: pub? enum<T>? Name { Variant, Variant(Type), ... }
fn try_parse_enum(chars: &[char], pos: &mut usize) -> Result<EnumDef, ParseError> {
    let start = *pos;

    // Check for "pub" keyword
    let visibility = if try_consume_keyword(chars, pos, "pub") {
        Visibility::Public
    } else {
        Visibility::Private
    };

    // Expect "enum"
    if !try_consume_keyword(chars, pos, "enum") {
        return Err(ParseError {
            message: "Expected 'enum' keyword".to_string(),
            location: Some(*pos),
        });
    }

    skip_whitespace(chars, pos);

    // Try to parse generic parameters BEFORE the name (e.g., enum<T>)
    let mut generic_params = Vec::new();
    if *pos < chars.len() && chars[*pos] == '<' {
        (*pos) += 1; // consume '<'
        loop {
            skip_whitespace(chars, pos);
            if *pos >= chars.len() {
                break;
            }
            if chars[*pos] == '>' {
                (*pos) += 1; // consume '>'
                break;
            }
            if chars[*pos] == ',' {
                (*pos) += 1; // consume ','
                continue;
            }
            let param = parse_ident(chars, pos)?;
            generic_params.push(param);
            skip_whitespace(chars, pos);
            if *pos >= chars.len() {
                break;
            }
            if chars[*pos] == '>' {
                (*pos) += 1; // consume '>'
                break;
            }
        }
    }

    skip_whitespace(chars, pos);

    // Parse enum name
    let name = parse_ident(chars, pos)?;

    // If no generic params before name, try after name (e.g., enum Name<T>)
    if generic_params.is_empty() {
        skip_whitespace(chars, pos);
        if *pos < chars.len() && chars[*pos] == '<' {
            (*pos) += 1; // consume '<'
            loop {
                skip_whitespace(chars, pos);
                if *pos >= chars.len() {
                    break;
                }
                if chars[*pos] == '>' {
                    (*pos) += 1; // consume '>'
                    break;
                }
                if chars[*pos] == ',' {
                    (*pos) += 1; // consume ','
                    continue;
                }
                let param = parse_ident(chars, pos)?;
                generic_params.push(param);
                skip_whitespace(chars, pos);
                if *pos >= chars.len() {
                    break;
                }
                if chars[*pos] == '>' {
                    (*pos) += 1; // consume '>'
                    break;
                }
            }
        }
    }

    // Expect "{"
    skip_whitespace(chars, pos);
    if !try_consume(chars, pos, '{') {
        return Err(ParseError {
            message: "Expected '{{'".to_string(),
            location: Some(*pos),
        });
    }

    // Parse variants and methods
    let mut variants = Vec::new();
    let mut methods = Vec::new();
    loop {
        skip_whitespace(chars, pos);
        if *pos >= chars.len() {
            break;
        }
        if try_consume(chars, pos, '}') {
            break;
        }

        // Check if it's a function definition (method)
        let method_start = *pos;
        if try_consume_keyword(chars, pos, "pub") || try_consume_keyword(chars, pos, "fn") {
            // It's a method, go back to method_start
            *pos = method_start;
            match try_parse_fn(chars, pos) {
                Ok(method) => {
                    methods.push(method);
                    skip_whitespace(chars, pos);
                    try_consume(chars, pos, ',');
                    continue;
                }
                Err(_) => {
                    // Not a valid method, try as variant
                }
            }
        } else if *pos < chars.len() && (chars[*pos].is_alphabetic() || chars[*pos] == '_') {
            // Check if it's "fn" keyword without pub
            let fn_start = *pos;
            let name = parse_ident(chars, pos)?;
            if name == "fn" {
                // It's a method, go back to fn_start
                *pos = fn_start;
                match try_parse_fn(chars, pos) {
                    Ok(method) => {
                        methods.push(method);
                        skip_whitespace(chars, pos);
                        try_consume(chars, pos, ',');
                        continue;
                    }
                    Err(_) => {}
                }
            }
            // It's a variant, go back to variant_start
            *pos = fn_start;
        }

        // Check for pub variant
        let variant_visibility = if try_consume_keyword(chars, pos, "pub") {
            Visibility::Public
        } else {
            Visibility::Private
        };

        // Parse variant name
        skip_whitespace(chars, pos);
        let variant_name = parse_ident(chars, pos)?;

        // Parse associated types (e.g., Variant(Type))
        let mut associated_types = Vec::new();
        skip_whitespace(chars, pos);
        if try_consume(chars, pos, '(') {
            loop {
                skip_whitespace(chars, pos);
                if *pos >= chars.len() {
                    break;
                }
                if try_consume(chars, pos, ')') {
                    break;
                }
                if try_consume(chars, pos, ',') {
                    continue;
                }
                let assoc_ty = parse_type(chars, pos)?;
                associated_types.push(assoc_ty);
                skip_whitespace(chars, pos);
                if try_consume(chars, pos, ')') {
                    break;
                }
            }
        }

        variants.push(EnumVariant {
            name: variant_name,
            associated_types,
            visibility: variant_visibility,
        });

        skip_whitespace(chars, pos);
        try_consume(chars, pos, ',');
    }

    // Consume optional semicolon after enum
    skip_whitespace(chars, pos);
    try_consume(chars, pos, ';');

    let span = span(start, *pos);

    Ok(EnumDef {
        name,
        variants,
        methods,
        visibility,
        generic_params,
        span,
    })
}

/// Parse a type (basic types or custom types)
fn parse_type(chars: &[char], pos: &mut usize) -> Result<Type, ParseError> {
    skip_whitespace(chars, pos);

    // Check for basic types
    if *pos < chars.len() {
        let remaining: String = chars[*pos..].iter().take(10).collect();
        if remaining.starts_with("i8") {
            *pos += 2;
            return Ok(Type::I8);
        } else if remaining.starts_with("i16") {
            *pos += 3;
            return Ok(Type::I16);
        } else if remaining.starts_with("i32") {
            *pos += 3;
            return Ok(Type::I32);
        } else if remaining.starts_with("i64") {
            *pos += 3;
            return Ok(Type::I64);
        } else if remaining.starts_with("u8") {
            *pos += 2;
            return Ok(Type::U8);
        } else if remaining.starts_with("u16") {
            *pos += 3;
            return Ok(Type::U16);
        } else if remaining.starts_with("u32") {
            *pos += 3;
            return Ok(Type::U32);
        } else if remaining.starts_with("u64") {
            *pos += 3;
            return Ok(Type::U64);
        } else if remaining.starts_with("bool") {
            *pos += 4;
            return Ok(Type::Bool);
        } else if remaining.starts_with("void") {
            *pos += 4;
            return Ok(Type::Void);
        }
    }

    // Parse custom type (identifier)
    let type_name = parse_ident(chars, pos)?;

    // Check for generic arguments (e.g., Vec<i64>)
    let mut generic_args = Vec::new();
    skip_whitespace(chars, pos);
    if try_consume(chars, pos, '<') {
        loop {
            skip_whitespace(chars, pos);
            if *pos >= chars.len() {
                break;
            }
            if try_consume(chars, pos, '>') {
                break;
            }
            if try_consume(chars, pos, ',') {
                continue;
            }
            let arg = parse_type(chars, pos)?;
            generic_args.push(arg);
            skip_whitespace(chars, pos);
            if try_consume(chars, pos, '>') {
                break;
            }
        }
    }

    Ok(Type::Custom {
        name: type_name,
        generic_args,
        is_exported: false,
    })
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
