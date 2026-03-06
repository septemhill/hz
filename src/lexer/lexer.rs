//! # Lexer implementation for Lang Programming Language
//!
//! This module contains the main Lexer struct and its implementation.

use super::error::LexerError;
use super::iterator::LexerIterator;
use super::token::{Token, TokenWithSpan};
use crate::ast::Span;

/// Lexer for the Lang programming language
///
/// This lexer can be used to tokenize source code into a collection of tokens.
pub struct Lexer {
    source: Vec<char>,
    pos: usize,
    done: bool,
}

impl Lexer {
    /// Create a new lexer from source code
    pub fn new(source: &str) -> Self {
        Lexer {
            source: source.chars().collect(),
            pos: 0,
            done: false,
        }
    }

    /// Tokenize the source code and return all tokens (legacy method)
    pub fn tokenize(mut self) -> Result<Vec<TokenWithSpan>, LexerError> {
        eprintln!(
            "DEBUG tokenize: source='{}'",
            self.source.iter().collect::<String>()
        );
        let mut tokens = Vec::new();

        while self.pos < self.source.len() {
            self.skip_whitespace();
            if self.pos >= self.source.len() {
                break;
            }

            let start = self.pos;
            let token = self.next_token()?;
            let end = self.pos;

            // Don't add whitespace or comment tokens, but add error tokens
            match &token {
                Token::Error(msg) => {
                    return Err(LexerError::new(&msg, start, file!(), line!()));
                }
                _ => {
                    // Skip whitespace and comments
                    if !matches!(token, Token::Eof) {
                        tokens.push(TokenWithSpan {
                            token,
                            span: Span { start, end },
                        });
                    }
                }
            }
        }

        // Add EOF token
        let end = self.pos;
        tokens.push(TokenWithSpan {
            token: Token::Eof,
            span: Span { start: end, end },
        });

        Ok(tokens)
    }

    /// Create an iterator from source code (convenience method)
    pub fn iter(source: &str) -> LexerIterator {
        LexerIterator::new(source)
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

    /// Get the next token
    fn next_token(&mut self) -> Result<Token, LexerError> {
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
                "==" => {
                    self.pos += 1; // Skip second character
                    return Ok(Token::Equal);
                }
                "!=" => {
                    self.pos += 1;
                    return Ok(Token::NotEqual);
                }
                "<=" => {
                    self.pos += 1;
                    return Ok(Token::LessEq);
                }
                ">=" => {
                    self.pos += 1;
                    return Ok(Token::GreaterEq);
                }
                "+=" => {
                    self.pos += 1;
                    return Ok(Token::PlusAssign);
                }
                "-=" => {
                    self.pos += 1;
                    return Ok(Token::MinusAssign);
                }
                "*=" => {
                    self.pos += 1;
                    return Ok(Token::StarAssign);
                }
                "/=" => {
                    self.pos += 1;
                    return Ok(Token::SlashAssign);
                }
                // "->" => return Ok(Token::Arrow),
                "&&" => {
                    self.pos += 1;
                    return Ok(Token::AmpAmp);
                }
                "||" => {
                    self.pos += 1;
                    return Ok(Token::PipePipe);
                }
                "<<" => {
                    self.pos += 1;
                    return Ok(Token::LessLess);
                }
                ">>" => {
                    self.pos += 1;
                    return Ok(Token::GreaterGreater);
                }
                "=>" => {
                    self.pos += 1;
                    return Ok(Token::FatArrow);
                }
                "//" => {
                    // This is a comment, skip it
                    while self.pos < self.source.len() && self.source[self.pos] != '\n' {
                        self.pos += 1;
                    }
                    return self.next_token();
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
            '.' => Ok(Token::Dot),
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

        // Check for keywords
        let result = match ident.as_str() {
            "fn" => Token::Fn,
            "pub" => Token::Pub,
            "var" => Token::Var,
            "const" => Token::Const,
            "let" => Token::Let,
            "return" => Token::Return,
            "import" => Token::Import,
            "struct" => Token::Struct,
            "enum" => Token::Enum,
            "if" => Token::If,
            "else" => Token::Else,
            "while" => Token::While,
            "loop" => Token::Loop,
            "true" => Token::True,
            "false" => Token::False,
            "null" => Token::Null,
            "switch" => Token::Switch,
            "self" => Token::SelfType,
            "external" => Token::External,
            "cdecl" => Token::Cdecl,
            "defer" => Token::Defer,
            "error" => {
                eprintln!("DEBUG LEXER: Found 'error' keyword!");
                Token::ErrorKw
            }
            "try" => Token::Try,
            "catch" => Token::Catch,
            "_" => Token::Underscore,
            _ => Token::Ident(ident.clone()),
        };
        eprintln!("DEBUG LEXER: ident='{}' => {:?}", ident, result);
        result
    }

    /// Read a number literal
    fn read_number(&mut self) -> Result<Token, LexerError> {
        let start = self.pos;

        while self.pos < self.source.len() && self.source[self.pos].is_ascii_digit() {
            self.pos += 1;
        }

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

    /// Read a character literal
    fn read_char(&mut self) -> Result<Token, LexerError> {
        let start = self.pos;
        self.pos += 1; // Consume '

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

/// Convenience function to tokenize source code
pub fn tokenize(source: &str) -> Result<Vec<TokenWithSpan>, LexerError> {
    Lexer::new(source).tokenize()
}
