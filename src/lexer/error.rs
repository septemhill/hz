//! # Lexer error types for Lang Programming Language
//!
//! This module defines the LexerError type used for error reporting.

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
