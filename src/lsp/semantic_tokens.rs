//! # Semantic Tokens for Colorization
//!
//! This module provides semantic token support for syntax highlighting.

use crate::ast::Span;
use crate::lexer::iterator::LexerIterator;
use crate::lexer::token::Token;

/// Semantic token types for the Lang language
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum SemanticTokenType {
    // General
    Namespace = 0,
    Type,
    Class,
    Enum,
    Interface,
    Struct,
    TypeParameter,
    Parameter,
    Variable,
    Property,
    EnumMember,
    Function,
    Method,
    Keyword,
    Modifier,
    Comment,
    String,
    Number,
    Operator,
    Punctuation,
    Decorator,
    // Lang-specific
    BuiltinType,
    BuiltinFunction,
    Label,
}

/// Semantic token modifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum SemanticTokenModifier {
    DefaultLibrary = 0,
    Declaration,
    Definition,
    Readonly,
    Static,
    Deprecated,
    Abstract,
    Async,
    Modification,
    Documentation,
}

/// Full semantic token result
#[derive(Debug, Clone)]
pub struct SemanticTokensResult {
    pub data: Vec<SemanticToken>,
}

/// A single semantic token
#[derive(Debug, Clone)]
pub struct SemanticToken {
    /// Delta line (relative to previous token)
    pub delta_line: u32,
    /// Delta start (relative to previous token on same line)
    pub delta_start: u32,
    /// Length of the token
    pub length: u32,
    /// Token type
    pub token_type: u32,
    /// Token modifiers (as bitflags)
    pub token_modifiers: u32,
}

/// Semantic token legend - maps token types to their names
pub fn token_legend() -> (Vec<&'static str>, Vec<&'static str>) {
    let token_types = vec![
        "namespace",
        "type",
        "class",
        "enum",
        "interface",
        "struct",
        "typeParameter",
        "parameter",
        "variable",
        "property",
        "enumMember",
        "function",
        "method",
        "keyword",
        "modifier",
        "comment",
        "string",
        "number",
        "operator",
        "punctuation",
        "decorator",
        // Lang-specific
        "builtinType",
        "builtinFunction",
        "label",
    ];

    let token_modifiers = vec![
        "defaultLibrary",
        "declaration",
        "definition",
        "readonly",
        "static",
        "deprecated",
        "abstract",
        "async",
        "modification",
        "documentation",
    ];

    (token_types, token_modifiers)
}

/// Get the token type for a Lexer token
fn get_token_type(token: &Token) -> Option<SemanticTokenType> {
    match token {
        // Keywords
        Token::Fn => Some(SemanticTokenType::Keyword),
        Token::Pub => Some(SemanticTokenType::Keyword),
        Token::Var => Some(SemanticTokenType::Keyword),
        Token::Const => Some(SemanticTokenType::Keyword),
        Token::Return => Some(SemanticTokenType::Keyword),
        Token::Import => Some(SemanticTokenType::Keyword),
        Token::Struct => Some(SemanticTokenType::Keyword),
        Token::Enum => Some(SemanticTokenType::Keyword),
        Token::If => Some(SemanticTokenType::Keyword),
        Token::Else => Some(SemanticTokenType::Keyword),
        Token::True => Some(SemanticTokenType::Keyword),
        Token::False => Some(SemanticTokenType::Keyword),
        Token::Null => Some(SemanticTokenType::Keyword),
        Token::For => Some(SemanticTokenType::Keyword),
        Token::Range => Some(SemanticTokenType::Keyword),
        Token::Switch => Some(SemanticTokenType::Keyword),
        Token::SelfType => Some(SemanticTokenType::Keyword),
        Token::External => Some(SemanticTokenType::Keyword),
        Token::Cdecl => Some(SemanticTokenType::Keyword),
        Token::Defer => Some(SemanticTokenType::Keyword),
        Token::DeferBang => Some(SemanticTokenType::Keyword),
        Token::ErrorKw => Some(SemanticTokenType::Keyword),
        Token::Try => Some(SemanticTokenType::Keyword),
        Token::Catch => Some(SemanticTokenType::Keyword),
        Token::Break => Some(SemanticTokenType::Keyword),
        Token::Continue => Some(SemanticTokenType::Keyword),
        Token::Interface => Some(SemanticTokenType::Keyword),
        Token::Impl => Some(SemanticTokenType::Keyword),
        Token::RawPtr => Some(SemanticTokenType::BuiltinType),

        // Built-in types
        Token::Ident(id) => {
            // Check for built-in types
            match id.as_str() {
                "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" | "f32" | "f64"
                | "bool" | "void" => Some(SemanticTokenType::BuiltinType),
                _ => Some(SemanticTokenType::Variable),
            }
        }

        // Literals
        Token::Int(_) => Some(SemanticTokenType::Number),
        Token::Float(_) => Some(SemanticTokenType::Number),
        Token::String(_) => Some(SemanticTokenType::String),
        Token::Char(_) => Some(SemanticTokenType::String),

        // Operators
        Token::Plus
        | Token::Minus
        | Token::Star
        | Token::Slash
        | Token::Percent
        | Token::Equal
        | Token::NotEqual
        | Token::Less
        | Token::Greater
        | Token::LessEq
        | Token::GreaterEq
        | Token::PlusAssign
        | Token::MinusAssign
        | Token::StarAssign
        | Token::SlashAssign
        | Token::Ampersand
        | Token::Pipe
        | Token::Underscore
        | Token::Not
        | Token::Caret
        | Token::AmpAmp
        | Token::PipePipe
        | Token::LessLess
        | Token::GreaterGreater
        | Token::Assign
        | Token::Question
        | Token::DotDot
        | Token::FatArrow => Some(SemanticTokenType::Operator),

        // Punctuation
        Token::LParen
        | Token::RParen
        | Token::LBrace
        | Token::RBrace
        | Token::LBracket
        | Token::RBracket
        | Token::Comma
        | Token::Semicolon
        | Token::Colon
        | Token::Dot => Some(SemanticTokenType::Punctuation),

        Token::Eof => None,
        Token::Error(_) => Some(SemanticTokenType::Variable),
    }
}

/// Compute line and column from byte position
fn position_from_byte_offset(content: &str, byte_offset: usize) -> (u32, u32) {
    let mut line: u32 = 0;
    let mut last_line_start: usize = 0;

    for (i, c) in content.char_indices() {
        if i >= byte_offset {
            break;
        }
        if c == '\n' {
            line += 1;
            last_line_start = i + 1;
        }
    }

    let column = byte_offset - last_line_start;
    (line, column as u32)
}

/// Tokenize the document and produce semantic tokens
pub fn compute_semantic_tokens(content: &str) -> Vec<SemanticToken> {
    let mut tokens = Vec::new();
    let mut lexer = LexerIterator::new(content);

    let mut prev_line: u32 = 0;
    let mut prev_start: u32 = 0;

    while let Some(result) = lexer.next() {
        let token_with_span = match result {
            Ok(t) => t,
            Err(_) => continue,
        };

        let token = &token_with_span.token;
        let span = token_with_span.span;

        if let Some(token_type) = get_token_type(token) {
            let (line, start) = position_from_byte_offset(content, span.start);
            let end = position_from_byte_offset(content, span.end).1;
            let length = end - start;

            let delta_line = line - prev_line;
            let delta_start = if delta_line == 0 {
                start - prev_start
            } else {
                start
            };

            tokens.push(SemanticToken {
                delta_line,
                delta_start,
                length,
                token_type: token_type as u32,
                token_modifiers: 0,
            });

            prev_line = line;
            prev_start = start;
        }
    }

    tokens
}

/// Convert semantic tokens to the LSP response format
pub fn tokens_to_response(tokens: Vec<SemanticToken>) -> serde_json::Value {
    let data: Vec<serde_json::Value> = tokens
        .into_iter()
        .flat_map(|t| {
            vec![
                serde_json::Value::Number(t.delta_line.into()),
                serde_json::Value::Number(t.delta_start.into()),
                serde_json::Value::Number(t.length.into()),
                serde_json::Value::Number(t.token_type.into()),
                serde_json::Value::Number(t.token_modifiers.into()),
            ]
        })
        .collect();

    serde_json::json!({ "data": data })
}
