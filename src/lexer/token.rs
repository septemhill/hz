//! # Token types for Lang Programming Language
//!
//! This module defines all token types used by the lexer.

use crate::ast::Span;

/// Token types for the Lang programming language
#[derive(Debug, Clone, PartialEq)]
#[allow(unused)]
pub enum Token {
    // Keywords
    Fn,
    Pub,
    Var,
    Const,
    Return,
    Import,
    Struct,
    Interface,
    Impl,
    Enum,
    If,
    Else,
    True,
    False,
    Null,
    For,
    Range,
    Switch,
    #[allow(non_camel_case_types)]
    SelfType,
    External,
    Cdecl,
    Defer,
    #[allow(non_camel_case_types)]
    DeferBang,
    ErrorKw,
    Try,
    Catch,
    Break,
    RawPtr,

    // Identifiers
    Ident(String),

    // Literals
    Int(i64),
    Float(f64),
    String(String),
    Char(char),

    // Symbols
    LParen,    // (
    RParen,    // )
    LBrace,    // {
    RBrace,    // }
    LBracket,  // [
    RBracket,  // ]
    Comma,     // ,
    Semicolon, // ;
    Colon,     // :
    Dot,       // .
    // Arrow,     // ->
    Question, // ?
    DotDot,   // ..

    // Operators
    Assign,         // =
    Plus,           // +
    Minus,          // -
    Star,           // *
    Slash,          // /
    Percent,        // %
    Equal,          // ==
    NotEqual,       // !=
    Less,           // <
    Greater,        // >
    LessEq,         // <=
    GreaterEq,      // >=
    PlusAssign,     // +=
    MinusAssign,    // -=
    StarAssign,     // *=
    SlashAssign,    // /=
    Ampersand,      // &
    Pipe,           // |
    Underscore,     // _
    Not,            // !
    Caret,          // ^
    AmpAmp,         // &&
    PipePipe,       // ||
    LessLess,       // <<
    GreaterGreater, // >>

    // End of file
    Eof,

    // Error
    Error(String),
    FatArrow, // =>
}

#[allow(unused)]
impl Token {
    /// Get the token type name for debugging
    pub fn type_name(&self) -> &'static str {
        match self {
            Token::Fn => "fn",
            Token::Pub => "pub",
            Token::Var => "var",
            Token::Const => "const",
            Token::Return => "return",
            Token::Import => "import",
            Token::Struct => "struct",
            Token::Interface => "interface",
            Token::Impl => "impl",
            Token::Enum => "enum",
            Token::If => "if",
            Token::Else => "else",
            Token::True => "true",
            Token::False => "false",
            Token::Null => "null",
            Token::Ident(_) => "ident",
            Token::Int(_) => "int",
            Token::Float(_) => "float",
            Token::String(_) => "string",
            Token::Char(_) => "char",
            Token::LParen => "(",
            Token::RParen => ")",
            Token::LBrace => "{",
            Token::RBrace => "}",
            Token::LBracket => "[",
            Token::RBracket => "]",
            Token::Comma => ",",
            Token::Semicolon => ";",
            Token::Colon => ":",
            Token::Dot => ".",
            // Token::Arrow => "->",
            Token::Question => "?",
            Token::DotDot => "..",
            Token::For => "for",
            Token::Range => "range",
            Token::Switch => "switch",
            Token::SelfType => "self",
            Token::External => "external",
            Token::Cdecl => "cdecl",
            Token::Defer => "defer",
            Token::DeferBang => "defer!",
            Token::ErrorKw => "error",
            Token::Try => "try",
            Token::Catch => "catch",
            Token::Break => "break",
            Token::RawPtr => "rawptr",
            Token::FatArrow => "=>",
            Token::Assign => "=",
            Token::Plus => "+",
            Token::Minus => "-",
            Token::Star => "*",
            Token::Slash => "/",
            Token::Percent => "%",
            Token::Equal => "==",
            Token::NotEqual => "!=",
            Token::Less => "<",
            Token::Greater => ">",
            Token::LessEq => "<=",
            Token::GreaterEq => ">=",
            Token::PlusAssign => "+=",
            Token::MinusAssign => "-=",
            Token::StarAssign => "*=",
            Token::SlashAssign => "/=",
            Token::Ampersand => "&",
            Token::Pipe => "|",
            Token::Caret => "^",
            Token::AmpAmp => "&&",
            Token::PipePipe => "||",
            Token::LessLess => "<<",
            Token::GreaterGreater => ">>",
            Token::Underscore => "_",
            Token::Not => "!",
            Token::Eof => "eof",
            Token::Error(_) => "error",
        }
    }
}

/// A token with position information
#[derive(Debug, Clone)]
pub struct TokenWithSpan {
    pub token: Token,
    pub span: Span,
}
