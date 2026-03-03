//! # Lexer for Lang Programming Language
//!
//! This module tokenizes source code into a stream of tokens for the parser.

use crate::ast::Span;

/// Token types for the Lang programming language
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // Keywords
    Fn,
    Pub,
    Var,
    Const,
    Let,
    Return,
    Import,
    Struct,
    Enum,
    If,
    Else,
    While,
    Loop,
    True,
    False,
    Null,

    // Identifiers
    Ident(String),

    // Literals
    Int(i64),
    String(String),

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

    // Operators
    Assign,      // =
    Plus,        // +
    Minus,       // -
    Star,        // *
    Slash,       // /
    Percent,     // %
    Equal,       // ==
    NotEqual,    // !=
    Less,        // <
    Greater,     // >
    LessEq,      // <=
    GreaterEq,   // >=
    PlusAssign,  // +=
    MinusAssign, // -=
    StarAssign,  // *=
    SlashAssign, // /=
    Ampersand,   // &
    Pipe,        // |
    Not,         // !

    // End of file
    Eof,

    // Error
    Error(String),
}

impl Token {
    /// Get the token type name for debugging
    pub fn type_name(&self) -> &'static str {
        match self {
            Token::Fn => "fn",
            Token::Pub => "pub",
            Token::Var => "var",
            Token::Const => "const",
            Token::Let => "let",
            Token::Return => "return",
            Token::Import => "import",
            Token::Struct => "struct",
            Token::Enum => "enum",
            Token::If => "if",
            Token::Else => "else",
            Token::While => "while",
            Token::Loop => "loop",
            Token::True => "true",
            Token::False => "false",
            Token::Null => "null",
            Token::Ident(_) => "ident",
            Token::Int(_) => "int",
            Token::String(_) => "string",
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

/// Lexer for the Lang programming language
///
/// This lexer can be used as an iterator to lazily produce tokens.
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
                    return Err(LexerError {
                        message: msg.clone(),
                        location: start,
                    });
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
                    return Ok(Token::Ampersand);
                }
                "||" => {
                    self.pos += 1;
                    return Ok(Token::Pipe);
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
            _ => Err(LexerError {
                message: format!("Unexpected character: '{}'", c),
                location: self.pos - 1,
            }),
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
        match ident.as_str() {
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
            _ => Token::Ident(ident),
        }
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
            Err(_) => Err(LexerError {
                message: format!("Invalid number: {}", num_str),
                location: start,
            }),
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
            return Err(LexerError {
                message: "Unterminated string literal".to_string(),
                location: start,
            });
        }

        Ok(Token::String(value))
    }
}

/// Iterator that lazily produces tokens from source code
///
/// This provides memory-efficient tokenization as tokens are generated on-demand.
pub struct LexerIterator {
    source: Vec<char>,
    pos: usize,
    done: bool,
    buffered: Option<TokenWithSpan>,
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

    /// Get the next token (internal, non-consuming)
    fn peek_token(&mut self) -> Result<Token, LexerError> {
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

        // Handle operators and symbols
        self.pos += 1;

        // Check for two-character operators first
        if self.pos < self.source.len() {
            let next = self.source[self.pos];
            let pair = format!("{}{}", c, next);

            match pair.as_str() {
                "==" => return Ok(Token::Equal),
                "!=" => return Ok(Token::NotEqual),
                "<=" => return Ok(Token::LessEq),
                ">=" => return Ok(Token::GreaterEq),
                "+=" => return Ok(Token::PlusAssign),
                "-=" => return Ok(Token::MinusAssign),
                "*=" => return Ok(Token::StarAssign),
                "/=" => return Ok(Token::SlashAssign),
                // "->" => return Ok(Token::Arrow),
                "&&" => return Ok(Token::Ampersand),
                "||" => return Ok(Token::Pipe),
                "//" => {
                    // This is a comment, skip it
                    while self.pos < self.source.len() && self.source[self.pos] != '\n' {
                        self.pos += 1;
                    }
                    return self.peek_token();
                }
                _ => {}
            }
        }

        // Single-character tokens
        // Note: Need to adjust pos back since we incremented
        let result = match c {
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
            _ => Err(LexerError {
                message: format!("Unexpected character: '{}'", c),
                location: self.pos - 1,
            }),
        };

        // Adjust position back by 1 since we're peeking, not consuming
        self.pos -= 1;
        result
    }

    /// Get the next token with span information
    fn next_token_inner(&mut self) -> Result<TokenWithSpan, LexerError> {
        if self.pos >= self.source.len() {
            return Ok(TokenWithSpan {
                token: Token::Eof,
                span: Span {
                    start: self.pos,
                    end: self.pos,
                },
            });
        }

        let start = self.pos;
        let token = self.peek_token()?;
        let end = self.pos + 1; // After advancing in next_token

        // Advance past the token
        self.pos += 1;

        // Handle multi-character tokens by advancing more
        match &token {
            Token::Ident(_) | Token::Int(_) => {
                while self.pos < self.source.len() {
                    let c = &self.source[self.pos];
                    if c.is_alphanumeric() || *c == '_' {
                        self.pos += 1;
                    } else {
                        break;
                    }
                }
            }
            Token::String(_) => {
                // String reading advances pos internally
            }
            _ => {}
        }

        Ok(TokenWithSpan {
            token,
            span: Span {
                start,
                end: self.pos,
            },
        })
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
        match ident.as_str() {
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
            _ => Token::Ident(ident),
        }
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
            Err(_) => Err(LexerError {
                message: format!("Invalid number: {}", num_str),
                location: start,
            }),
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
            return Err(LexerError {
                message: "Unterminated string literal".to_string(),
                location: start,
            });
        }

        Ok(Token::String(value))
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

        match self.peek_token() {
            Ok(token) => {
                let start = self.pos;
                let end = self.pos + 1;

                // Advance past the token
                self.pos += 1;

                // Handle multi-character tokens
                match &token {
                    Token::Ident(_) | Token::Int(_) => {
                        while self.pos < self.source.len() {
                            let c = &self.source[self.pos];
                            if c.is_alphanumeric() || *c == '_' {
                                self.pos += 1;
                            } else {
                                break;
                            }
                        }
                    }
                    Token::String(_) => {
                        // String reading advances pos internally
                        // Skip opening quote
                        if self.pos < self.source.len() && self.source[self.pos] == '"' {
                            self.pos += 1;
                        }
                        while self.pos < self.source.len() && self.source[self.pos] != '"' {
                            if self.source[self.pos] == '\\' && self.pos + 1 < self.source.len() {
                                self.pos += 2;
                            } else {
                                self.pos += 1;
                            }
                        }
                        // Skip closing quote
                        if self.pos < self.source.len() && self.source[self.pos] == '"' {
                            self.pos += 1;
                        }
                    }
                    _ => {
                        // Handle two-character operators
                        if self.pos < self.source.len() {
                            let c = self.source[self.pos - 1];
                            let next = self.source[self.pos];
                            let pair = format!("{}{}", c, next);
                            match pair.as_str() {
                                "==" | "!=" | "<=" | ">=" | "+=" | "-=" | "*=" | "/=" | "->"
                                | "&&" | "||" | "//" => {
                                    self.pos += 1;
                                }
                                _ => {}
                            }
                        }
                    }
                }

                let actual_end = self.pos;
                Some(Ok(TokenWithSpan {
                    token,
                    span: Span {
                        start,
                        end: actual_end,
                    },
                }))
            }
            Err(e) => {
                self.done = true;
                Some(Err(e))
            }
        }
    }
}

/// A peekable iterator for the lexer that allows looking at tokens without consuming them
pub struct PeekableLexerIterator {
    iter: LexerIterator,
    peeked: Option<Result<TokenWithSpan, LexerError>>,
}

impl PeekableLexerIterator {
    /// Create a new peekable lexer iterator from source code
    pub fn new(source: &str) -> Self {
        PeekableLexerIterator {
            iter: LexerIterator::new(source),
            peeked: None,
        }
    }

    /// Get the next token without consuming it
    pub fn peek(&mut self) -> Option<&TokenWithSpan> {
        if self.peeked.is_none() {
            self.peeked = self.iter.next();
        }

        match &self.peeked {
            Some(Ok(token)) => Some(token),
            _ => None,
        }
    }

    /// Get the next token without consuming it, including errors
    pub fn peek_result(&mut self) -> Option<Result<&TokenWithSpan, &LexerError>> {
        if self.peeked.is_none() {
            self.peeked = self.iter.next();
        }

        match &self.peeked {
            Some(Ok(token)) => Some(Ok(token)),
            Some(Err(e)) => Some(Err(e)),
            None => None,
        }
    }

    /// Peek at the nth token (0 = current, 1 = next, etc.)
    pub fn peek_n(&mut self, n: usize) -> Option<&TokenWithSpan> {
        if n == 0 {
            return self.peek();
        }

        // Consume peeked token first if available
        if let Some(result) = self.peeked.take() {
            match result {
                Ok(token) => {
                    // We need to consume tokens to peek ahead
                    // For simplicity, advance and try again
                    drop(token);
                    self.peek_n(n - 1)
                }
                Err(_) => None,
            }
        } else {
            // Advance and peek at n-1
            self.next();
            self.peek_n(n - 1)
        }
    }

    /// Consume the peeked token (if any) and return the next token
    pub fn next(&mut self) -> Option<Result<TokenWithSpan, LexerError>> {
        // Return peeked token if available
        if let Some(peeked) = self.peeked.take() {
            return Some(peeked);
        }

        // Otherwise get next from iterator
        self.iter.next()
    }

    /// Check if we're at the end of input
    pub fn is_at_end(&mut self) -> bool {
        self.peek().is_none() || matches!(self.peek(), Some(tok) if matches!(tok.token, Token::Eof))
    }
}

/// Convenience function to tokenize source code as a peekable iterator
pub fn iter(source: &str) -> PeekableLexerIterator {
    PeekableLexerIterator::new(source)
}

/// Lexer error type
#[derive(Debug)]
pub struct LexerError {
    pub message: String,
    pub location: usize,
}

impl std::fmt::Display for LexerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Lexer error at position {}: {}",
            self.location, self.message
        )
    }
}

impl std::error::Error for LexerError {}

/// Convenience function to tokenize source code
pub fn tokenize(source: &str) -> Result<Vec<TokenWithSpan>, LexerError> {
    Lexer::new(source).tokenize()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper function to read example file contents
    fn read_example(filename: &str) -> String {
        std::fs::read_to_string(filename).unwrap_or_default()
    }

    // Test basic keywords
    #[test]
    fn test_keyword_fn() {
        let result = tokenize("fn").unwrap();
        assert_eq!(result.len(), 2); // fn + EOF
        assert_eq!(result[0].token, Token::Fn);
    }

    #[test]
    fn test_keyword_let() {
        let result = tokenize("let").unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].token, Token::Let);
    }

    #[test]
    fn test_keyword_var() {
        let result = tokenize("var").unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].token, Token::Var);
    }

    #[test]
    fn test_keyword_const() {
        let result = tokenize("const").unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].token, Token::Const);
    }

    #[test]
    fn test_keyword_return() {
        let result = tokenize("return").unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].token, Token::Return);
    }

    #[test]
    fn test_keyword_if() {
        let result = tokenize("if").unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].token, Token::If);
    }

    #[test]
    fn test_keyword_else() {
        let result = tokenize("else").unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].token, Token::Else);
    }

    #[test]
    fn test_keyword_while() {
        let result = tokenize("while").unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].token, Token::While);
    }

    #[test]
    fn test_keyword_loop() {
        let result = tokenize("loop").unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].token, Token::Loop);
    }

    #[test]
    fn test_keyword_struct() {
        let result = tokenize("struct").unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].token, Token::Struct);
    }

    #[test]
    fn test_keyword_enum() {
        let result = tokenize("enum").unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].token, Token::Enum);
    }

    #[test]
    fn test_keyword_import() {
        let result = tokenize("import").unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].token, Token::Import);
    }

    #[test]
    fn test_keyword_pub() {
        let result = tokenize("pub").unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].token, Token::Pub);
    }

    #[test]
    fn test_keyword_true() {
        let result = tokenize("true").unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].token, Token::True);
    }

    #[test]
    fn test_keyword_false() {
        let result = tokenize("false").unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].token, Token::False);
    }

    #[test]
    fn test_keyword_null() {
        let result = tokenize("null").unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].token, Token::Null);
    }

    // Test identifiers
    #[test]
    fn test_identifier() {
        let result = tokenize("myVariable").unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].token, Token::Ident("myVariable".to_string()));
    }

    #[test]
    fn test_identifier_with_underscore() {
        let result = tokenize("_private_var").unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].token, Token::Ident("_private_var".to_string()));
    }

    // Test numbers
    #[test]
    fn test_integer() {
        let result = tokenize("42").unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].token, Token::Int(42));
    }

    #[test]
    fn test_integer_zero() {
        let result = tokenize("0").unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].token, Token::Int(0));
    }

    #[test]
    fn test_integer_large() {
        let result = tokenize("1234567890").unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].token, Token::Int(1234567890));
    }

    // Test strings
    #[test]
    fn test_string() {
        let result = tokenize("\"hello\"").unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].token, Token::String("hello".to_string()));
    }

    #[test]
    fn test_string_with_escape() {
        let result = tokenize("\"hello\\nworld\"").unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].token, Token::String("hello\nworld".to_string()));
    }

    // Test operators
    #[test]
    fn test_operators() {
        let result = tokenize("+ - * / %").unwrap();
        assert_eq!(result.len(), 6); // 5 operators + EOF
        assert_eq!(result[0].token, Token::Plus);
        assert_eq!(result[1].token, Token::Minus);
        assert_eq!(result[2].token, Token::Star);
        assert_eq!(result[3].token, Token::Slash);
        assert_eq!(result[4].token, Token::Percent);
    }

    #[test]
    fn test_comparison_operators() {
        let result = tokenize("==!=<><=>=").unwrap();
        assert_eq!(result.len(), 7); // 6 operators + EOF
        assert_eq!(result[0].token, Token::Equal);
        assert_eq!(result[1].token, Token::NotEqual);
        assert_eq!(result[2].token, Token::Less);
        assert_eq!(result[3].token, Token::Greater);
        assert_eq!(result[4].token, Token::LessEq);
        assert_eq!(result[5].token, Token::GreaterEq);
    }

    #[test]
    fn test_assignment_operators() {
        let result = tokenize("=+=-=*=/=").unwrap();
        assert_eq!(result.len(), 6); // 5 operators + EOF
        assert_eq!(result[0].token, Token::Assign);
        assert_eq!(result[1].token, Token::PlusAssign);
        assert_eq!(result[2].token, Token::MinusAssign);
        assert_eq!(result[3].token, Token::StarAssign);
        assert_eq!(result[4].token, Token::SlashAssign);
    }

    // Test symbols
    #[test]
    fn test_symbols() {
        let result = tokenize("(){}[][],;: .?").unwrap();
        assert_eq!(result.len(), 14); // 13 symbols + EOF (space is ignored)
        assert_eq!(result[0].token, Token::LParen);
        assert_eq!(result[1].token, Token::RParen);
        assert_eq!(result[2].token, Token::LBrace);
        assert_eq!(result[3].token, Token::RBrace);
        assert_eq!(result[4].token, Token::LBracket);
        assert_eq!(result[5].token, Token::RBracket);
        assert_eq!(result[6].token, Token::LBracket);
        assert_eq!(result[7].token, Token::RBracket);
        assert_eq!(result[8].token, Token::Comma);
        assert_eq!(result[9].token, Token::Semicolon);
        assert_eq!(result[10].token, Token::Colon);
        assert_eq!(result[11].token, Token::Dot);
        assert_eq!(result[12].token, Token::Question);
    }

    // #[test]
    // fn test_arrow() {
    //     let result = tokenize("->").unwrap();
    //     assert_eq!(result.len(), 2);
    //     assert_eq!(result[0].token, Token::Arrow);
    // }

    #[test]
    fn test_logical_operators() {
        let result = tokenize("&|!").unwrap();
        assert_eq!(result.len(), 4); // 3 operators + EOF
        assert_eq!(result[0].token, Token::Ampersand);
        assert_eq!(result[1].token, Token::Pipe);
        assert_eq!(result[2].token, Token::Not);
    }

    // Test comments
    #[test]
    fn test_single_line_comment() {
        let result = tokenize("// this is a comment\n42").unwrap();
        assert_eq!(result.len(), 2); // number + EOF (comment skipped)
        assert_eq!(result[0].token, Token::Int(42));
    }

    // Test example files
    #[test]
    fn test_example_simple() {
        let source = read_example("examples/test_simple.lang");
        let result = tokenize(&source);
        assert!(
            result.is_ok(),
            "Failed to tokenize test_simple.lang: {:?}",
            result.err()
        );
        let tokens = result.unwrap();
        // Should have at least: fn, main, (, ), i64, {, return, 42, }, EOF
        assert!(tokens.len() > 5);
    }

    #[test]
    fn test_example_return_simple() {
        let source = read_example("examples/test_return_simple.lang");
        let result = tokenize(&source);
        assert!(
            result.is_ok(),
            "Failed to tokenize test_return_simple.lang: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_example_add_literal() {
        let source = read_example("examples/test_add_literal.lang");
        let result = tokenize(&source);
        assert!(
            result.is_ok(),
            "Failed to tokenize test_add_literal.lang: {:?}",
            result.err()
        );
        let tokens = result.unwrap();
        // Check for return keyword and numbers
        let has_return = tokens.iter().any(|t| matches!(t.token, Token::Return));
        assert!(has_return);
    }

    #[test]
    fn test_example_tuple() {
        let source = read_example("examples/test_tuple.lang");
        let result = tokenize(&source);
        assert!(
            result.is_ok(),
            "Failed to tokenize test_tuple.lang: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_example_import() {
        let source = read_example("examples/test_import_group.lang");
        let result = tokenize(&source);
        assert!(
            result.is_ok(),
            "Failed to tokenize test_import_group.lang: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_example_features() {
        let source = read_example("examples/test_features.lang");
        let result = tokenize(&source);
        assert!(
            result.is_ok(),
            "Failed to tokenize test_features.lang: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_example_destructure() {
        let source = read_example("examples/test_destructure_simple.lang");
        let result = tokenize(&source);
        assert!(
            result.is_ok(),
            "Failed to tokenize test_destructure_simple.lang: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_example_var_reassign() {
        let source = read_example("examples/test_var_reassign.lang");
        let result = tokenize(&source);
        assert!(
            result.is_ok(),
            "Failed to tokenize test_var_reassign.lang: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_example_optional() {
        let source = read_example("examples/test_optional_simple.lang");
        let result = tokenize(&source);
        assert!(
            result.is_ok(),
            "Failed to tokenize test_optional_simple.lang: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_example_interface() {
        let source = read_example("examples/test_interface.lang");
        let result = tokenize(&source);
        assert!(
            result.is_ok(),
            "Failed to tokenize test_interface.lang: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_example_void_no_arrow() {
        let source = read_example("examples/test_void_no_arrow.lang");
        let result = tokenize(&source);
        assert!(
            result.is_ok(),
            "Failed to tokenize test_void_no_arrow.lang: {:?}",
            result.err()
        );
    }

    // Test error handling
    #[test]
    fn test_unexpected_character() {
        let result = tokenize("@");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("Unexpected character"));
    }

    #[test]
    fn test_unterminated_string() {
        let result = tokenize("\"hello");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("Unterminated"));
    }

    // Test combined tokenization
    #[test]
    fn test_function_definition() {
        let source = "fn main() i64 { return 42; }";
        let result = tokenize(source).unwrap();
        let types: Vec<&str> = result.iter().map(|t| t.token.type_name()).collect();
        assert!(types.contains(&"fn"));
        assert!(types.contains(&"ident")); // main
        // i64 is treated as identifier (not a keyword in this lexer)
        assert!(types.iter().filter(|&&t| t == "ident").count() >= 2);
        assert!(types.contains(&"return"));
        assert!(types.contains(&"int"));
    }

    #[test]
    fn test_tuple_syntax() {
        let source = "(1, 2, 3)";
        let result = tokenize(source).unwrap();
        assert_eq!(result.len(), 8); // (, 1, ,, 2, ,, 3, ), EOF
    }

    #[test]
    fn test_tuple_access() {
        let source = "t.0";
        let result = tokenize(source).unwrap();
        assert_eq!(result.len(), 4); // ident, ., int, EOF
    }

    #[test]
    fn test_type_annotation() {
        let source = "x: i64";
        let result = tokenize(source).unwrap();
        let types: Vec<&str> = result.iter().map(|t| t.token.type_name()).collect();
        assert!(types.contains(&"ident")); // x
        assert!(types.contains(&":"));
        assert!(types.contains(&"ident")); // i64
    }

    #[test]
    fn test_whitespace_handling() {
        let source = "fn    main   (  )   i64   {   }";
        let result = tokenize(source).unwrap();
        assert!(result.len() > 0);
    }
}
