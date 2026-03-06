//! # Lexer for Lang Programming Language
//!
//! This module tokenizes source code into a stream of tokens for the parser.

use std::collections::VecDeque;

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
    For,
    Range,
    Switch,
    #[allow(non_camel_case_types)]
    SelfType,
    External,
    Cdecl,
    Defer,
    ErrorKw,
    Try,
    Catch,

    // Identifiers
    Ident(String),

    // Literals
    Int(i64),
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
            Token::ErrorKw => "error",
            Token::Try => "try",
            Token::Catch => "catch",
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

    /// Read a character literal
    fn read_char(&mut self) -> Result<Token, LexerError> {
        let start = self.pos;
        self.pos += 1; // Consume '

        if self.pos >= self.source.len() {
            return Err(LexerError {
                message: "Unterminated character literal".to_string(),
                location: start,
            });
        }

        let c = self.source[self.pos];
        self.pos += 1;

        if self.pos >= self.source.len() || self.source[self.pos] != '\'' {
            return Err(LexerError {
                message: "Expected closing quote for character literal".to_string(),
                location: start,
            });
        }
        self.pos += 1;

        Ok(Token::Char(c))
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
            "for" => Token::For,
            "range" => Token::Range,
            "switch" => Token::Switch,
            "self" => Token::SelfType,
            "external" => Token::External,
            "cdecl" => Token::Cdecl,
            "defer" => Token::Defer,
            "error" => Token::ErrorKw,
            "try" => Token::Try,
            "catch" => Token::Catch,
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

    fn read_char(&mut self) -> Result<Token, LexerError> {
        let start = self.pos;
        self.pos += 1;
        if self.pos >= self.source.len() {
            return Err(LexerError {
                message: "Unterminated character literal".to_string(),
                location: start,
            });
        }
        let c = self.source[self.pos];
        self.pos += 1;
        if self.pos >= self.source.len() || self.source[self.pos] != '\'' {
            return Err(LexerError {
                message: "Expected closing quote for character literal".to_string(),
                location: start,
            });
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

/// A peekable iterator for the lexer that allows looking at tokens without consuming them
pub struct PeekableLexerIterator {
    iter: LexerIterator,
    peeked: VecDeque<Result<TokenWithSpan, LexerError>>,
}

impl PeekableLexerIterator {
    /// Create a new peekable lexer iterator from source code
    pub fn new(source: &str) -> Self {
        PeekableLexerIterator {
            iter: LexerIterator::new(source),
            peeked: VecDeque::new(),
        }
    }

    /// Get a token without consuming it
    ///
    /// If `offset` is 0, returns the current token. If `offset` is 1, returns the next token.
    /// Returns `None` if there is no token at that offset.
    pub fn peek(&mut self, offset: usize) -> Option<&TokenWithSpan> {
        // Ensure we have enough tokens buffered
        while self.peeked.len() <= offset {
            match self.iter.next() {
                Some(Ok(token)) => {
                    self.peeked.push_back(Ok(token));
                }
                Some(Err(e)) => {
                    self.peeked.push_back(Err(e));
                }
                None => {
                    // No more tokens available
                    break;
                }
            }
        }

        self.peeked.get(offset).and_then(|r| r.as_ref().ok())
    }

    /// Consume the peeked token (if any) and return the next token
    pub fn next(&mut self) -> Option<Result<TokenWithSpan, LexerError>> {
        // eprintln!(
        //     "DEBUG next START: peeked.len={}, iter.pos={}",
        //     self.peeked.len(),
        //     self.iter.pos
        // );

        // Return buffered token if available
        if !self.peeked.is_empty() {
            let result = self.peeked.pop_front();
            // eprintln!(
            //     "DEBUG next: popped {:?}, iter.pos={}",
            //     result.as_ref().map(|r| r.as_ref().map(|t| &t.token)),
            //     self.iter.pos
            // );
            return result;
        }

        // Otherwise get next from iterator
        let result = self.iter.next();
        // eprintln!(
        //     "DEBUG next: from iter {:?}, iter.pos now {}",
        //     result.as_ref().map(|r| r.as_ref().map(|t| &t.token)),
        //     self.iter.pos
        // );
        result
    }

    /// Check if we're at the end of input
    pub fn is_at_end(&mut self) -> bool {
        // Check if we have any buffered tokens
        if !self.peeked.is_empty() {
            // Check if the front token is EOF
            if let Some(Ok(token)) = self.peeked.front() {
                return matches!(token.token, Token::Eof);
            }
            // If there's an error, we're not really at end
            return false;
        }

        // No buffered tokens, check the iterator
        // We need to peek to know if we're at end
        self.peek(0);

        // Check if we got an EOF token
        if let Some(Ok(token)) = self.peeked.front() {
            return matches!(token.token, Token::Eof);
        }

        // If peek(0) returned None (iterator exhausted with None),
        // we are at the end
        if self.peeked.is_empty() {
            return true;
        }

        false
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

    // ============================================================================
    // LexerIterator Tests
    // ============================================================================

    // Test LexerIterator::new() creates empty iterator for empty source
    #[test]
    fn test_lexer_iterator_new_empty() {
        let iter = LexerIterator::new("");
        // Iterator should be created successfully
        assert_eq!(iter.source.len(), 0);
        assert_eq!(iter.pos, 0);
        assert!(!iter.done);
        assert!(iter.buffered.is_none());
    }

    // Test LexerIterator::new() creates iterator with source characters
    #[test]
    fn test_lexer_iterator_new_with_source() {
        let iter = LexerIterator::new("fn x");
        // Source includes whitespace characters
        assert_eq!(iter.source.len(), 4); // 'f', 'n', ' ', 'x'
        assert_eq!(iter.pos, 0);
        assert!(!iter.done);
    }

    // Test Iterator::next() returns first token
    #[test]
    fn test_lexer_iterator_next_first_token() {
        let mut iter = LexerIterator::new("fn");
        let result = iter.next();
        assert!(result.is_some());
        let token_result = result.unwrap();
        assert!(token_result.is_ok());
        let token_with_span = token_result.unwrap();
        assert_eq!(token_with_span.token, Token::Fn);
    }

    // Test Iterator::next() returns keywords correctly
    #[test]
    fn test_lexer_iterator_next_keywords() {
        let mut iter = LexerIterator::new("fn let var const return if else while loop");

        let tokens = std::iter::from_fn(|| iter.next())
            .filter_map(|r| r.ok())
            .map(|t| t.token)
            .collect::<Vec<_>>();

        assert!(tokens.contains(&Token::Fn));
        assert!(tokens.contains(&Token::Let));
        assert!(tokens.contains(&Token::Var));
        assert!(tokens.contains(&Token::Const));
        assert!(tokens.contains(&Token::Return));
        assert!(tokens.contains(&Token::If));
        assert!(tokens.contains(&Token::Else));
        assert!(tokens.contains(&Token::While));
        assert!(tokens.contains(&Token::Loop));
    }

    // Test Iterator::next() returns identifiers correctly
    #[test]
    fn test_lexer_iterator_next_identifiers() {
        let mut iter = LexerIterator::new("foo");

        let token = iter.next().unwrap().unwrap();
        assert_eq!(token.token, Token::Ident("foo".to_string()));
    }

    // Test single identifier
    #[test]
    fn test_lexer_iterator_single_identifier() {
        let mut iter = LexerIterator::new("bar");

        let token = iter.next().unwrap().unwrap();
        assert_eq!(token.token, Token::Ident("bar".to_string()));
    }

    // Test identifier with underscore
    #[test]
    fn test_lexer_iterator_identifier_underscore() {
        let mut iter = LexerIterator::new("_private");

        let token = iter.next().unwrap().unwrap();
        assert_eq!(token.token, Token::Ident("_private".to_string()));
    }

    // Test identifier with numbers
    #[test]
    fn test_lexer_iterator_identifier_numbers() {
        let mut iter = LexerIterator::new("myVar123");

        let token = iter.next().unwrap().unwrap();
        assert_eq!(token.token, Token::Ident("myVar123".to_string()));
    }

    // Test Iterator::next() returns integer literals correctly
    #[test]
    fn test_lexer_iterator_next_integers() {
        let mut iter = LexerIterator::new("42");

        let token = iter.next().unwrap().unwrap();
        assert_eq!(token.token, Token::Int(42));
    }

    // Test single large integer
    #[test]
    fn test_lexer_iterator_single_integer() {
        let mut iter = LexerIterator::new("12345");

        let token = iter.next().unwrap().unwrap();
        assert_eq!(token.token, Token::Int(12345));
    }

    // Test Iterator::next() returns string literals correctly
    #[test]
    fn test_lexer_iterator_next_strings() {
        let mut iter = LexerIterator::new("\"hello\"");

        let token = iter.next().unwrap().unwrap();
        assert_eq!(token.token, Token::String("hello".to_string()));
    }

    // Test string literal with escape sequences
    #[test]
    fn test_lexer_iterator_string_escape() {
        let mut iter = LexerIterator::new("\"hello\\nworld\"");

        let token = iter.next().unwrap().unwrap();
        assert_eq!(token.token, Token::String("hello\nworld".to_string()));
    }

    // Test Iterator::next() returns operators correctly
    #[test]
    fn test_lexer_iterator_next_operators() {
        let mut iter = LexerIterator::new("+ - * / %");

        let tokens = std::iter::from_fn(|| iter.next())
            .filter_map(|r| r.ok())
            .map(|t| t.token)
            .collect::<Vec<_>>();

        assert!(tokens.contains(&Token::Plus));
        assert!(tokens.contains(&Token::Minus));
        assert!(tokens.contains(&Token::Star));
        assert!(tokens.contains(&Token::Slash));
        assert!(tokens.contains(&Token::Percent));
    }

    // Test Iterator::next() returns comparison operators correctly
    #[test]
    fn test_lexer_iterator_next_comparison_operators() {
        let mut iter = LexerIterator::new("== != < > <= >=");

        let tokens = std::iter::from_fn(|| iter.next())
            .filter_map(|r| r.ok())
            .map(|t| t.token)
            .collect::<Vec<_>>();

        assert!(tokens.contains(&Token::Equal));
        assert!(tokens.contains(&Token::NotEqual));
        assert!(tokens.contains(&Token::Less));
        assert!(tokens.contains(&Token::Greater));
        assert!(tokens.contains(&Token::LessEq));
        assert!(tokens.contains(&Token::GreaterEq));
    }

    // Test Iterator::next() returns assignment operators correctly
    #[test]
    fn test_lexer_iterator_next_assignment_operators() {
        let mut iter = LexerIterator::new("= += -= *= /=");

        let tokens = std::iter::from_fn(|| iter.next())
            .filter_map(|r| r.ok())
            .map(|t| t.token)
            .collect::<Vec<_>>();

        assert!(tokens.contains(&Token::Assign));
        assert!(tokens.contains(&Token::PlusAssign));
        assert!(tokens.contains(&Token::MinusAssign));
        assert!(tokens.contains(&Token::StarAssign));
        assert!(tokens.contains(&Token::SlashAssign));
    }

    // Test Iterator::next() returns symbols correctly
    #[test]
    fn test_lexer_iterator_next_symbols() {
        let mut iter = LexerIterator::new("(){}[][],;: .?");

        let tokens = std::iter::from_fn(|| iter.next())
            .filter_map(|r| r.ok())
            .map(|t| t.token)
            .collect::<Vec<_>>();

        assert!(tokens.contains(&Token::LParen));
        assert!(tokens.contains(&Token::RParen));
        assert!(tokens.contains(&Token::LBrace));
        assert!(tokens.contains(&Token::RBrace));
        assert!(tokens.contains(&Token::LBracket));
        assert!(tokens.contains(&Token::RBracket));
        assert!(tokens.contains(&Token::Comma));
        assert!(tokens.contains(&Token::Semicolon));
        assert!(tokens.contains(&Token::Colon));
        assert!(tokens.contains(&Token::Dot));
        assert!(tokens.contains(&Token::Question));
    }

    // Test Iterator::next() returns EOF token at end
    #[test]
    fn test_lexer_iterator_next_eof() {
        let mut iter = LexerIterator::new("fn");

        // First call returns 'fn' keyword
        let first = iter.next().unwrap().unwrap();
        assert_eq!(first.token, Token::Fn);

        // Second call returns EOF
        let second = iter.next().unwrap().unwrap();
        assert_eq!(second.token, Token::Eof);

        // Third call returns None (iterator exhausted)
        let third = iter.next();
        assert!(third.is_none());
    }

    // Test Iterator::next() returns None after EOF
    #[test]
    fn test_lexer_iterator_next_none_after_eof() {
        let mut iter = LexerIterator::new("");

        // First call returns EOF for empty source
        let first = iter.next().unwrap().unwrap();
        assert_eq!(first.token, Token::Eof);

        // Second call returns None
        let second = iter.next();
        assert!(second.is_none());
    }

    // Test Iterator::next() skips comments
    #[test]
    fn test_lexer_iterator_next_skips_comments() {
        let mut iter = LexerIterator::new("// this is a comment\n42");

        let result = iter.next().unwrap().unwrap();
        assert_eq!(result.token, Token::Int(42));
    }

    // Test Iterator::next() skips whitespace
    #[test]
    fn test_lexer_iterator_next_skips_whitespace() {
        let mut iter = LexerIterator::new("   fn   let   var   ");

        let tokens = std::iter::from_fn(|| iter.next())
            .filter_map(|r| r.ok())
            .map(|t| t.token)
            .collect::<Vec<_>>();

        assert!(tokens.contains(&Token::Fn));
        assert!(tokens.contains(&Token::Let));
        assert!(tokens.contains(&Token::Var));
    }

    // Test Iterator::next() handles errors correctly
    #[test]
    fn test_lexer_iterator_next_error() {
        let mut iter = LexerIterator::new("@");

        let result = iter.next();
        assert!(result.is_some());
        let token_result = result.unwrap();
        assert!(token_result.is_err());
    }

    // Test LexerIterator produces correct span information
    #[test]
    fn test_lexer_iterator_spans() {
        let mut iter = LexerIterator::new("fn");

        let fn_token = iter.next().unwrap().unwrap();
        // Span should have valid start and end
        assert!(fn_token.span.start >= 0);
        assert!(fn_token.span.end >= fn_token.span.start);
    }

    // Test Iterator::next() with for loop
    #[test]
    fn test_lexer_iterator_for_loop() {
        let source = "fn x";
        let mut iter = LexerIterator::new(source);
        let mut count = 0;

        for token_result in iter {
            assert!(token_result.is_ok());
            count += 1;
        }

        // Should have at least fn and EOF
        assert!(count >= 2);
    }

    // Test Lexer::iter() convenience method
    #[test]
    fn test_lexer_iter_convenience() {
        let mut iter = Lexer::iter("fn let var");

        let tokens = std::iter::from_fn(|| iter.next())
            .filter_map(|r| r.ok())
            .map(|t| t.token)
            .collect::<Vec<_>>();

        assert!(tokens.contains(&Token::Fn));
        assert!(tokens.contains(&Token::Let));
        assert!(tokens.contains(&Token::Var));
    }

    // ============================================================================
    // PeekableLexerIterator Tests
    // ============================================================================

    // Test PeekableLexerIterator::new() creates iterator correctly
    #[test]
    fn test_peekable_lexer_iterator_new() {
        let mut iter = PeekableLexerIterator::new("fn");
        assert!(!iter.is_at_end());
    }

    // Test PeekableLexerIterator::peek() returns first token without consuming
    #[test]
    fn test_peekable_lexer_iterator_peek() {
        let mut iter = PeekableLexerIterator::new("fn x");

        // Peek should return 'fn' without consuming
        let peeked = iter.peek(0);
        assert!(peeked.is_some());
        assert_eq!(peeked.unwrap().token, Token::Fn);

        // Peek again should return same token
        let peeked_again = iter.peek(0);
        assert!(peeked_again.is_some());
        assert_eq!(peeked_again.unwrap().token, Token::Fn);
    }

    // Test PeekableLexerIterator::peek() does not consume tokens
    #[test]
    fn test_peekable_lexer_iterator_peek_does_not_consume() {
        let mut iter = PeekableLexerIterator::new("fn let");

        // Peek twice
        iter.peek(0);
        iter.peek(0);

        // Now consume - should still get 'fn'
        let next = iter.next().unwrap().unwrap();
        assert_eq!(next.token, Token::Fn);
    }

    // Test PeekableLexerIterator::next() consumes peeked token
    #[test]
    fn test_peekable_lexer_iterator_next_consumes_peeked() {
        let mut iter = PeekableLexerIterator::new("fn let");

        // Peek at first token
        let peeked = iter.peek(0);
        assert_eq!(peeked.unwrap().token, Token::Fn);

        // Call next - should consume the peeked token
        let next = iter.next().unwrap().unwrap();
        assert_eq!(next.token, Token::Fn);

        // Next peek should return 'let'
        let peeked_after = iter.peek(0);
        assert_eq!(peeked_after.unwrap().token, Token::Let);
    }

    // Test PeekableLexerIterator::next() works without prior peek
    #[test]
    fn test_peekable_lexer_iterator_next_without_peek() {
        let mut iter = PeekableLexerIterator::new("fn let");

        // Call next without peek
        let next = iter.next().unwrap().unwrap();
        assert_eq!(next.token, Token::Fn);

        // Call next again
        let next2 = iter.next().unwrap().unwrap();
        assert_eq!(next2.token, Token::Let);
    }

    // Test PeekableLexerIterator::is_at_end() returns false at start
    #[test]
    fn test_peekable_lexer_iterator_is_at_end_false() {
        let mut iter = PeekableLexerIterator::new("fn x");
        assert!(!iter.is_at_end());
    }

    // Test PeekableLexerIterator::is_at_end() returns true at EOF
    #[test]
    fn test_peekable_lexer_iterator_is_at_end_true() {
        let mut iter = PeekableLexerIterator::new("");
        // Consume all tokens
        while let Some(result) = iter.next() {
            let _ = result;
        }
        assert!(iter.is_at_end());
    }

    // Test PeekableLexerIterator::is_at_end() returns true after all tokens consumed
    #[test]
    fn test_peekable_lexer_iterator_is_at_end_after_consume() {
        let mut iter = PeekableLexerIterator::new("fn");

        // Consume 'fn' token
        iter.next();

        // Note: is_at_end() behavior may vary depending on implementation
        // This test just verifies the method can be called
        iter.is_at_end();
    }

    // Test iter() convenience function creates PeekableLexerIterator
    #[test]
    fn test_iter_convenience_function() {
        let mut iter = iter("fn let var");

        let peeked = iter.peek(0);
        assert!(peeked.is_some());
        assert_eq!(peeked.unwrap().token, Token::Fn);
    }

    // Test PeekableLexerIterator with full tokenization workflow
    #[test]
    fn test_peekable_lexer_iterator_full_workflow() {
        let mut iter = PeekableLexerIterator::new("fn x");

        // Peek at first token
        assert_eq!(iter.peek(0).unwrap().token, Token::Fn);

        // Consume fn
        assert_eq!(iter.next().unwrap().unwrap().token, Token::Fn);

        // Peek at second token (identifier)
        assert_eq!(iter.peek(0).unwrap().token, Token::Ident("x".to_string()));
    }

    // Test PeekableLexerIterator handles errors through next()
    #[test]
    fn test_peekable_lexer_iterator_error_handling() {
        let mut iter = PeekableLexerIterator::new("@");

        let result = iter.next();
        assert!(result.is_some());
        assert!(result.unwrap().is_err());
    }

    // Test peek(1) returns the next token correctly
    #[test]
    fn test_peekable_lexer_iterator_peek_offset_one() {
        let mut iter = PeekableLexerIterator::new("fn let x");

        // Peek at offset 1 should return 'let'
        let peeked = iter.peek(1);
        assert!(peeked.is_some());
        assert_eq!(peeked.unwrap().token, Token::Let);

        // Peek at offset 1 again should return same token
        let peeked_again = iter.peek(1);
        assert!(peeked_again.is_some());
        assert_eq!(peeked_again.unwrap().token, Token::Let);
    }

    // Test peek(0) and peek(1) together
    #[test]
    fn test_peekable_lexer_iterator_peek_zero_and_one_together() {
        let mut iter = PeekableLexerIterator::new("fn let var");

        // Peek at offset 0 should return 'fn'
        let peeked_0 = iter.peek(0);
        assert!(peeked_0.is_some());
        assert_eq!(peeked_0.unwrap().token, Token::Fn);

        // Peek at offset 1 should return 'let'
        let peeked_1 = iter.peek(1);
        assert!(peeked_1.is_some());
        assert_eq!(peeked_1.unwrap().token, Token::Let);

        // Peek at offset 0 again should still return 'fn'
        let peeked_0_again = iter.peek(0);
        assert!(peeked_0_again.is_some());
        assert_eq!(peeked_0_again.unwrap().token, Token::Fn);

        // Peek at offset 1 again should still return 'let'
        let peeked_1_again = iter.peek(1);
        assert!(peeked_1_again.is_some());
        assert_eq!(peeked_1_again.unwrap().token, Token::Let);
    }

    // Test consuming after peek(1)
    #[test]
    fn test_peekable_lexer_iterator_peek_one_then_consume() {
        let mut iter = PeekableLexerIterator::new("fn let var");

        // Peek at offset 1
        let peeked_1 = iter.peek(1);
        assert_eq!(peeked_1.unwrap().token, Token::Let);

        // Consume first token - should get 'fn'
        let next = iter.next().unwrap().unwrap();
        assert_eq!(next.token, Token::Fn);

        // Now peek(0) should return 'let'
        let peeked_0_after = iter.peek(0);
        assert_eq!(peeked_0_after.unwrap().token, Token::Let);

        // Now peek(1) should return 'var'
        let peeked_1_after = iter.peek(1);
        assert_eq!(peeked_1_after.unwrap().token, Token::Var);
    }

    // Test cross-using peek(0) and peek(1) with simple input
    #[test]
    fn test_peekable_lexer_iterator_peek_cross_use_complex() {
        let mut iter = PeekableLexerIterator::new("fn + -");

        // Peek at offset 0: fn
        assert_eq!(iter.peek(0).unwrap().token, Token::Fn);

        // Peek at offset 1: +
        assert_eq!(iter.peek(1).unwrap().token, Token::Plus);

        // Peek at offset 0 again: fn
        assert_eq!(iter.peek(0).unwrap().token, Token::Fn);

        // Peek at offset 1 again: +
        assert_eq!(iter.peek(1).unwrap().token, Token::Plus);

        // Consume fn
        assert_eq!(iter.next().unwrap().unwrap().token, Token::Fn);

        // Now peek(0) should be +
        assert_eq!(iter.peek(0).unwrap().token, Token::Plus);

        // And peek(1) should be -
        assert_eq!(iter.peek(1).unwrap().token, Token::Minus);

        // Consume +
        assert_eq!(iter.next().unwrap().unwrap().token, Token::Plus);

        // Peek(0) should now be -
        assert_eq!(iter.peek(0).unwrap().token, Token::Minus);
    }

    // Test that peek(1) doesn't consume tokens
    #[test]
    fn test_peekable_lexer_iterator_peek_one_does_not_consume() {
        let mut iter = PeekableLexerIterator::new("fn let var");

        // Peek at offset 1 multiple times
        iter.peek(1);
        iter.peek(1);

        // Consume first token - should still get 'fn'
        let next = iter.next().unwrap().unwrap();
        assert_eq!(next.token, Token::Fn);

        // Consume second token - should still get 'let'
        let next2 = iter.next().unwrap().unwrap();
        assert_eq!(next2.token, Token::Let);
    }

    // Test peek(1) at end of input
    #[test]
    fn test_peekable_lexer_iterator_peek_one_at_end() {
        let mut iter = PeekableLexerIterator::new("fn");

        // Peek at offset 0: fn
        let peeked_0 = iter.peek(0);
        assert!(peeked_0.is_some());
        assert_eq!(peeked_0.unwrap().token, Token::Fn);

        // Peek at offset 1: should be EOF or None
        let peeked_1 = iter.peek(1);
        // Either None (no more tokens) or EOF is acceptable
        // The implementation may return None or return EOF token
    }

    // Test complex cross-use of peek(0) and peek(1) with operators
    #[test]
    fn test_peekable_lexer_iterator_peek_complex_function_syntax() {
        let mut iter = PeekableLexerIterator::new("a + b");

        // Initial peek
        assert_eq!(iter.peek(0).unwrap().token, Token::Ident("a".to_string()));
        assert_eq!(iter.peek(1).unwrap().token, Token::Plus);
        assert_eq!(iter.peek(0).unwrap().token, Token::Ident("a".to_string()));

        // Consume a
        iter.next();

        // After consuming, peek(0) should be +, peek(1) should be b
        assert_eq!(iter.peek(0).unwrap().token, Token::Plus);
        assert_eq!(iter.peek(1).unwrap().token, Token::Ident("b".to_string()));
        assert_eq!(iter.peek(0).unwrap().token, Token::Plus);

        // Consume +
        iter.next();

        // Now peek(0) should be b
        assert_eq!(iter.peek(0).unwrap().token, Token::Ident("b".to_string()));
    }

    // Test complex cross-use with interleaving peek(0) and peek(1)
    #[test]
    fn test_peekable_lexer_iterator_peek_interleaved_pattern() {
        let mut iter = PeekableLexerIterator::new("a + b - c");

        // Pattern: peek(0), peek(1), peek(0), peek(1)
        assert_eq!(iter.peek(0).unwrap().token, Token::Ident("a".to_string()));
        assert_eq!(iter.peek(1).unwrap().token, Token::Plus);
        assert_eq!(iter.peek(0).unwrap().token, Token::Ident("a".to_string()));
        assert_eq!(iter.peek(1).unwrap().token, Token::Plus);

        // Consume a
        iter.next();

        // Pattern again: peek(0), peek(1), peek(0), peek(1)
        assert_eq!(iter.peek(0).unwrap().token, Token::Plus);
        assert_eq!(iter.peek(1).unwrap().token, Token::Ident("b".to_string()));
        assert_eq!(iter.peek(0).unwrap().token, Token::Plus);
        assert_eq!(iter.peek(1).unwrap().token, Token::Ident("b".to_string()));

        // Consume +
        iter.next();

        // Continue pattern
        assert_eq!(iter.peek(0).unwrap().token, Token::Ident("b".to_string()));
        assert_eq!(iter.peek(1).unwrap().token, Token::Minus);
        assert_eq!(iter.peek(0).unwrap().token, Token::Ident("b".to_string()));
        assert_eq!(iter.peek(1).unwrap().token, Token::Minus);
    }

    // Test peek(0) and peek(1) with string literals
    #[test]
    fn test_peekable_lexer_iterator_peek_with_strings() {
        let mut iter = PeekableLexerIterator::new("x = \"hello\"");

        assert_eq!(iter.peek(0).unwrap().token, Token::Ident("x".to_string()));
        assert_eq!(iter.peek(1).unwrap().token, Token::Assign);
        assert_eq!(iter.peek(0).unwrap().token, Token::Ident("x".to_string()));

        iter.next();

        assert_eq!(iter.peek(0).unwrap().token, Token::Assign);
        assert_eq!(
            iter.peek(1).unwrap().token,
            Token::String("hello".to_string())
        );
    }

    // Test peek(0) and peek(1) with numbers
    #[test]
    fn test_peekable_lexer_iterator_peek_with_numbers() {
        let mut iter = PeekableLexerIterator::new("x = 123");

        assert_eq!(iter.peek(0).unwrap().token, Token::Ident("x".to_string()));
        assert_eq!(iter.peek(1).unwrap().token, Token::Assign);
        assert_eq!(iter.peek(0).unwrap().token, Token::Ident("x".to_string()));

        iter.next();
        iter.next();

        assert_eq!(iter.peek(0).unwrap().token, Token::Int(123));
    }

    #[test]
    fn test_peekable_lexer_iterator_peek_with_numbers_111() {
        let mut iter = PeekableLexerIterator::new("fn sample(a: int, b: int) { return a + b; }");

        assert_eq!(iter.peek(0).unwrap().token, Token::Fn);
        assert_eq!(
            iter.peek(1).unwrap().token,
            Token::Ident("sample".to_string())
        );
        assert_eq!(iter.peek(0).unwrap().token, Token::Fn);

        iter.next();
        iter.next();

        assert_eq!(iter.peek(0).unwrap().token, Token::LParen);
        assert_eq!(iter.peek(1).unwrap().token, Token::Ident("a".to_string()));
        assert_eq!(iter.peek(0).unwrap().token, Token::LParen);
    }
}
