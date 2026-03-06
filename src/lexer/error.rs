//! # Lexer error types for Lang Programming Language
//!
//! This module defines the LexerError type used for error reporting.

/// Lexer error type
#[derive(Debug)]
pub struct LexerError {
    pub message: String,
    pub location: usize,
    pub file_name: String,
    pub line: u32,
}

impl std::fmt::Display for LexerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}: Lexer error at position {}: {}",
            self.file_name, self.line, self.location, self.message
        )
    }
}

impl std::error::Error for LexerError {}

impl LexerError {
    /// Create a new lexer error with file name and line number
    pub fn new(message: &str, location: usize, file_name: &str, line: u32) -> Self {
        LexerError {
            message: message.to_string(),
            location,
            file_name: file_name.to_string(),
            line,
        }
    }
}
