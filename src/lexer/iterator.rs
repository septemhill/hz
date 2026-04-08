//! # Lexer Iterator for Lang Programming Language
//!
//! This module contains the LexerIterator which provides lazy tokenization.

use super::error::LexerError;
use super::token::{Token, TokenWithSpan};
use crate::ast::Span;

/// Iterator that lazily produces tokens from source code
///
/// This provides memory-efficient tokenization as tokens are generated on-demand.
#[allow(unused)]
pub struct LexerIterator<'a> {
    pub source: &'a str,
    pub pos: usize,
    pub done: bool,
    pub buffered: Option<TokenWithSpan>,
}

impl<'a> LexerIterator<'a> {
    /// Create a new lexer iterator from source code
    pub fn new(source: &'a str) -> Self {
        LexerIterator {
            source,
            pos: 0,
            done: false,
            buffered: None,
        }
    }

    fn source_len(&self) -> usize {
        self.source.len()
    }

    fn current_char(&self) -> Option<char> {
        self.source[self.pos..].chars().next()
    }

    fn peek_char(&self) -> Option<char> {
        self.source[self.pos..].chars().nth(1)
    }

    /// Skip whitespace characters
    fn skip_whitespace(&mut self) {
        while let Some(c) = self.current_char() {
            if !c.is_whitespace() {
                break;
            }
            self.pos += 1;
        }

        // Skip single-line comments
        if let (Some(c1), Some(c2)) = (self.current_char(), self.peek_char()) {
            if c1 == '/' && c2 == '/' {
                while let Some(c) = self.current_char() {
                    if c == '\n' {
                        break;
                    }
                    self.pos += 1;
                }
                // Recursively skip more whitespace after comment
                self.skip_whitespace();
            }
        }
    }

    /// Get the next token (internal, advances position)
    fn read_token(&mut self) -> Result<Token, LexerError> {
        // Skip whitespace first
        self.skip_whitespace();

        if self.pos >= self.source_len() {
            return Ok(Token::Eof);
        }

        let c = match self.current_char() {
            Some(c) => c,
            None => return Ok(Token::Eof),
        };

        // Handle identifiers and keywords

        // Handle identifiers and keywords
        if c.is_alphabetic() || c == '_' || c == '@' {
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
        if let Some(next) = self.current_char() {
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
                    while let Some(ch) = self.current_char() {
                        if ch == '\n' {
                            break;
                        }
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
                if self.current_char() == Some('.') {
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

        while let Some(c) = self.current_char() {
            if c.is_alphanumeric() || c == '_' || c == '@' {
                self.pos += 1;
            } else {
                break;
            }
        }

        let ident: String = self.source[start..self.pos].to_string();

        // Special case: check for "defer!" - if we have "defer" followed by "!", include the "!"
        // This must be checked before the keyword matching
        if ident == "defer" && self.current_char() == Some('!') {
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
            "interface" => Token::Interface,
            "impl" => Token::Impl,
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
            "extern" => Token::Extern,
            "defer" => Token::Defer,
            "defer!" => Token::DeferBang,
            "error" => Token::ErrorKw,
            "try" => Token::Try,
            "catch" => Token::Catch,
            "break" => Token::Break,
            "continue" => Token::Continue,
            "rawptr" => Token::RawPtr,
            "varargs" => Token::VarArgs,
            "inline" => Token::Inline,
            _ => Token::Ident(ident),
        }
    }

    /// Read a number literal
    fn read_number(&mut self) -> Result<Token, LexerError> {
        let start = self.pos;

        // Read integer part
        while let Some(c) = self.current_char() {
            if c.is_ascii_digit() {
                self.pos += 1;
            } else {
                break;
            }
        }

        // Check for floating point
        if self.current_char() == Some('.') {
            let dot_pos = self.pos;
            self.pos += 1; // consume the dot

            // Check if there's a fractional part
            if let Some(c) = self.current_char() {
                if c.is_ascii_digit() {
                    // Read fractional part
                    while let Some(c) = self.current_char() {
                        if c.is_ascii_digit() {
                            self.pos += 1;
                        } else {
                            break;
                        }
                    }

                    let num_str: String = self.source[start..self.pos].to_string();
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
                    let num_str: String = self.source[start..self.pos].to_string();
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
                // It's just a dot, not a float - back up
                self.pos = dot_pos;
                let num_str: String = self.source[start..self.pos].to_string();
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
            let num_str: String = self.source[start..self.pos].to_string();
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

        while let Some(c) = self.current_char() {
            if c == '"' {
                break;
            }

            // Handle escape sequences
            if c == '\\' {
                self.pos += 1;
                let next = match self.current_char() {
                    Some(n) => n,
                    None => {
                        return Err(LexerError::new(
                            "Unterminated string literal",
                            start,
                            file!(),
                            line!(),
                        ));
                    }
                };
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
        if self.current_char() == Some('"') {
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
        if self.pos >= self.source_len() {
            return Err(LexerError::new(
                "Unterminated character literal",
                start,
                file!(),
                line!(),
            ));
        }
        let c = match self.current_char() {
            Some(ch) => ch,
            None => {
                return Err(LexerError::new(
                    "Unterminated character literal",
                    start,
                    file!(),
                    line!(),
                ));
            }
        };
        self.pos += 1;
        if self.pos >= self.source_len() || self.current_char() != Some('\'') {
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

impl Iterator for LexerIterator<'_> {
    type Item = Result<TokenWithSpan, LexerError>;

    fn next(&mut self) -> Option<Self::Item> {
        // Skip whitespace before returning next token
        self.skip_whitespace();

        if self.pos >= self.source_len() {
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
