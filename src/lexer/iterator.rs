//! # Lexer Iterator for Lang Programming Language
//!
//! This module contains the LexerIterator which provides lazy tokenization.

use super::error::LexerError;
use super::token::{Token, TokenWithSpan};
use crate::ast::Span;
use std::collections::VecDeque;

/// Iterator that lazily produces tokens from source code
///
/// This provides memory-efficient tokenization as tokens are generated on-demand.
pub struct LexerIterator {
    pub source: Vec<char>,
    pub pos: usize,
    pub done: bool,
    pub buffered: Option<TokenWithSpan>,
}

impl LexerIterator {
    /// Create a new lexer iterator from source code
    pub fn new(source: &str) -> Self {
        LexerIterator {
            source: source.chars().collect(),
            pos: 0,
            done: false,
            buffered: None,
        }
    }

    /// Skip whitespace characters
    fn skip_whitespace(&mut self) {
        while self.pos < self.source.len() && self.source[self.pos].is_whitespace() {
            self.pos += 1;
        }

        // Skip single-line comments
        if self.pos + 1 < self.source.len()
            && self.source[self.pos] == '/'
            && self.source[self.pos + 1] == '/'
        {
            while self.pos < self.source.len() && self.source[self.pos] != '\n' {
                self.pos += 1;
            }
            // Recursively skip more whitespace after comment
            self.skip_whitespace();
        }
    }

    /// Get the next token (internal, advances position)
    fn read_token(&mut self) -> Result<Token, LexerError> {
        // Skip whitespace first
        self.skip_whitespace();

        if self.pos >= self.source.len() {
            return Ok(Token::Eof);
        }

        let c = self.source[self.pos];

        // Handle identifiers and keywords
        if c.is_alphabetic() || c == '_' {
            return Ok(self.read_identifier_or_keyword());
        }

        // Handle numbers
        if c.is_ascii_digit() {
            return self.read_number();
        }

        // Handle strings
        if c == '"' {
            return self.read_string();
        }
        if c == '\'' {
            return self.read_char();
        }

        // Handle operators and symbols
        self.pos += 1;

        // Check for two-character operators first
        if self.pos < self.source.len() {
            let next = self.source[self.pos];
            let pair = format!("{}{}", c, next);

            match pair.as_str() {
                "==" | "!=" | "<=" | ">=" | "+=" | "-=" | "*=" | "/=" | "&&" | "||" | "<<"
                | ">>" | "=>" => {
                    self.pos += 1; // Skip second character
                    return Ok(match pair.as_str() {
                        "==" => Token::Equal,
                        "!=" => Token::NotEqual,
                        "<=" => Token::LessEq,
                        ">=" => Token::GreaterEq,
                        "+=" => Token::PlusAssign,
                        "-=" => Token::MinusAssign,
                        "*=" => Token::StarAssign,
                        "/=" => Token::SlashAssign,
                        "&&" => Token::AmpAmp,
                        "||" => Token::PipePipe,
                        "<<" => Token::LessLess,
                        ">>" => Token::GreaterGreater,
                        "=>" => Token::FatArrow,
                        _ => unreachable!(),
                    });
                }
                "//" => {
                    // This is a comment, skip it
                    while self.pos < self.source.len() && self.source[self.pos] != '\n' {
                        self.pos += 1;
                    }
                    return self.read_token();
                }
                _ => {}
            }
        }

        // Single-character tokens
        match c {
            '(' => Ok(Token::LParen),
            ')' => Ok(Token::RParen),
            '{' => Ok(Token::LBrace),
            '}' => Ok(Token::RBrace),
            '[' => Ok(Token::LBracket),
            ']' => Ok(Token::RBracket),
            ',' => Ok(Token::Comma),
            ';' => Ok(Token::Semicolon),
            ':' => Ok(Token::Colon),
            '.' => {
                if self.pos < self.source.len() && self.source[self.pos] == '.' {
                    self.pos += 1;
                    Ok(Token::DotDot)
                } else {
                    Ok(Token::Dot)
                }
            }
            '?' => Ok(Token::Question),
            '=' => Ok(Token::Assign),
            '+' => Ok(Token::Plus),
            '-' => Ok(Token::Minus),
            '*' => Ok(Token::Star),
            '/' => Ok(Token::Slash),
            '%' => Ok(Token::Percent),
            '<' => Ok(Token::Less),
            '>' => Ok(Token::Greater),
            '&' => Ok(Token::Ampersand),
            '|' => Ok(Token::Pipe),
            '!' => Ok(Token::Not),
            _ => Err(LexerError::new(
                &format!("Unexpected character: '{}'", c),
                self.pos - 1,
                file!(),
                line!(),
            )),
        }
    }

    /// Read an identifier or keyword
    fn read_identifier_or_keyword(&mut self) -> Token {
        let start = self.pos;

        while self.pos < self.source.len() {
            let c = &self.source[self.pos];
            if c.is_alphanumeric() || *c == '_' {
                self.pos += 1;
            } else {
                break;
            }
        }

        let ident: String = self.source[start..self.pos].iter().collect();

        // Special case: check for "defer!" - if we have "defer" followed by "!", include the "!"
        // This must be checked before the keyword matching
        if ident == "defer" && self.pos < self.source.len() && self.source[self.pos] == '!' {
            self.pos += 1; // consume the '!'
            return Token::DeferBang;
        }

        // Check for keywords
        match ident.as_str() {
            "fn" => Token::Fn,
            "pub" => Token::Pub,
            "var" => Token::Var,
            "const" => Token::Const,
            "return" => Token::Return,
            "import" => Token::Import,
            "struct" => Token::Struct,
            "enum" => Token::Enum,
            "if" => Token::If,
            "else" => Token::Else,
            "true" => Token::True,
            "false" => Token::False,
            "null" => Token::Null,
            "for" => Token::For,
            "range" => Token::Range,
            "switch" => Token::Switch,
            "self" => Token::SelfType,
            "external" => Token::External,
            "cdecl" => Token::Cdecl,
            "defer" => Token::Defer,
            "defer!" => Token::DeferBang,
            "error" => Token::ErrorKw,
            "try" => Token::Try,
            "catch" => Token::Catch,
            "break" => Token::Break,
            _ => Token::Ident(ident),
        }
    }

    /// Read a number literal
    fn read_number(&mut self) -> Result<Token, LexerError> {
        let start = self.pos;

        // Read integer part
        while self.pos < self.source.len() && self.source[self.pos].is_ascii_digit() {
            self.pos += 1;
        }

        // Check for floating point
        if self.pos < self.source.len() && self.source[self.pos] == '.' {
            let dot_pos = self.pos;
            self.pos += 1; // consume the dot

            // Check if there's a fractional part
            if self.pos < self.source.len() && self.source[self.pos].is_ascii_digit() {
                // Read fractional part
                while self.pos < self.source.len() && self.source[self.pos].is_ascii_digit() {
                    self.pos += 1;
                }

                let num_str: String = self.source[start..self.pos].iter().collect();
                match num_str.parse::<f64>() {
                    Ok(value) => Ok(Token::Float(value)),
                    Err(_) => Err(LexerError::new(
                        &format!("Invalid float: {}", num_str),
                        start,
                        file!(),
                        line!(),
                    )),
                }
            } else {
                // It's just a dot, not a float - back up
                self.pos = dot_pos;
                let num_str: String = self.source[start..self.pos].iter().collect();
                match num_str.parse::<i64>() {
                    Ok(value) => Ok(Token::Int(value)),
                    Err(_) => Err(LexerError::new(
                        &format!("Invalid number: {}", num_str),
                        start,
                        file!(),
                        line!(),
                    )),
                }
            }
        } else {
            let num_str: String = self.source[start..self.pos].iter().collect();
            match num_str.parse::<i64>() {
                Ok(value) => Ok(Token::Int(value)),
                Err(_) => Err(LexerError::new(
                    &format!("Invalid number: {}", num_str),
                    start,
                    file!(),
                    line!(),
                )),
            }
        }
    }

    /// Read a string literal
    fn read_string(&mut self) -> Result<Token, LexerError> {
        let start = self.pos;

        // Consume opening quote
        self.pos += 1;

        let mut value = String::new();

        while self.pos < self.source.len() && self.source[self.pos] != '"' {
            let c = self.source[self.pos];

            // Handle escape sequences
            if c == '\\' && self.pos + 1 < self.source.len() {
                self.pos += 1;
                let next = self.source[self.pos];
                match next {
                    'n' => value.push('\n'),
                    't' => value.push('\t'),
                    'r' => value.push('\r'),
                    '\\' => value.push('\\'),
                    '"' => value.push('"'),
                    _ => value.push(next),
                }
            } else {
                value.push(c);
            }

            self.pos += 1;
        }

        // Consume closing quote
        if self.pos < self.source.len() && self.source[self.pos] == '"' {
            self.pos += 1;
        } else {
            return Err(LexerError::new(
                "Unterminated string literal",
                start,
                file!(),
                line!(),
            ));
        }

        Ok(Token::String(value))
    }

    fn read_char(&mut self) -> Result<Token, LexerError> {
        let start = self.pos;
        self.pos += 1;
        if self.pos >= self.source.len() {
            return Err(LexerError::new(
                "Unterminated character literal",
                start,
                file!(),
                line!(),
            ));
        }
        let c = self.source[self.pos];
        self.pos += 1;
        if self.pos >= self.source.len() || self.source[self.pos] != '\'' {
            return Err(LexerError::new(
                "Expected closing quote for character literal",
                start,
                file!(),
                line!(),
            ));
        }
        self.pos += 1;
        Ok(Token::Char(c))
    }
}

impl Iterator for LexerIterator {
    type Item = Result<TokenWithSpan, LexerError>;

    fn next(&mut self) -> Option<Self::Item> {
        // Skip whitespace before returning next token
        self.skip_whitespace();

        if self.pos >= self.source.len() {
            if !self.done {
                self.done = true;
                return Some(Ok(TokenWithSpan {
                    token: Token::Eof,
                    span: Span {
                        start: self.pos,
                        end: self.pos,
                    },
                }));
            }
            return None;
        }

        let start = self.pos;
        match self.read_token() {
            Ok(token) => {
                let end = self.pos;
                Some(Ok(TokenWithSpan {
                    token,
                    span: Span { start, end },
                }))
            }
            Err(e) => {
                self.done = true;
                Some(Err(e))
            }
        }
    }
}
