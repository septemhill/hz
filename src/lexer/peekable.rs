//! # Peekable Lexer Iterator for Lang Programming Language
//!
//! This module contains the PeekableLexerIterator which allows peeking at tokens
//! without consuming them.

use super::error::LexerError;
use super::iterator::LexerIterator;
use super::token::TokenWithSpan;
use std::collections::VecDeque;

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
                return matches!(token.token, super::token::Token::Eof);
            }
            // If there's an error, we're not really at end
            return false;
        }

        // No buffered tokens, check the iterator
        // We need to peek to know if we're at end
        self.peek(0);

        // Check if we got an EOF token
        if let Some(Ok(token)) = self.peeked.front() {
            return matches!(token.token, super::token::Token::Eof);
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
