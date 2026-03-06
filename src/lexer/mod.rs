//! # Lexer for Lang Programming Language
//!
//! This module tokenizes source code into a stream of tokens for the parser.

pub mod error;
pub mod iterator;
pub mod lexer;
pub mod peekable;
pub mod token;

#[cfg(test)]
mod tests;

pub use error::LexerError;
pub use iterator::LexerIterator;
pub use lexer::{Lexer, tokenize};
pub use peekable::{PeekableLexerIterator, iter};
pub use token::{Token, TokenWithSpan};
