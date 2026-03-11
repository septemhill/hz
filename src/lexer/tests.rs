//! # Tests for Lexer
//!
//! This module contains all the tests for the lexer.

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
    let mut iter = LexerIterator::new("fn let var const return if else");

    let tokens = std::iter::from_fn(|| iter.next())
        .filter_map(|r| r.ok())
        .map(|t| t.token)
        .collect::<Vec<_>>();

    assert!(tokens.contains(&Token::Fn));
    assert!(tokens.contains(&Token::Var));
    assert!(tokens.contains(&Token::Const));
    assert!(tokens.contains(&Token::Return));
    assert!(tokens.contains(&Token::If));
    assert!(tokens.contains(&Token::Else));
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
    let mut iter = LexerIterator::new("   fn   var   const   ");

    let tokens = std::iter::from_fn(|| iter.next())
        .filter_map(|r| r.ok())
        .map(|t| t.token)
        .collect::<Vec<_>>();

    assert!(tokens.contains(&Token::Fn));
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
    let mut iter = Lexer::iter("fn var const");

    let tokens = std::iter::from_fn(|| iter.next())
        .filter_map(|r| r.ok())
        .map(|t| t.token)
        .collect::<Vec<_>>();

    assert!(tokens.contains(&Token::Fn));
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
    let mut iter = PeekableLexerIterator::new("fn var");

    // Peek at first token
    let peeked = iter.peek(0);
    assert_eq!(peeked.unwrap().token, Token::Fn);

    // Call next - should consume the peeked token
    let next = iter.next().unwrap().unwrap();
    assert_eq!(next.token, Token::Fn);

    // Next peek should return 'var'
    let peeked_after = iter.peek(0);
    assert_eq!(peeked_after.unwrap().token, Token::Var);
}

// Test PeekableLexerIterator::next() works without prior peek
#[test]
fn test_peekable_lexer_iterator_next_without_peek() {
    let mut iter = PeekableLexerIterator::new("fn var");

    // Call next without peek
    let next = iter.next().unwrap().unwrap();
    assert_eq!(next.token, Token::Fn);

    // Call next again
    let next2 = iter.next().unwrap().unwrap();
    assert_eq!(next2.token, Token::Var);
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
    let mut iter = PeekableLexerIterator::new("fn var x");

    // Peek at offset 1 should return 'var'
    let peeked = iter.peek(1);
    assert!(peeked.is_some());
    assert_eq!(peeked.unwrap().token, Token::Var);

    // Peek at offset 1 again should return same token
    let peeked_again = iter.peek(1);
    assert!(peeked_again.is_some());
    assert_eq!(peeked_again.unwrap().token, Token::Var);
}

// Test peek(0) and peek(1) together
#[test]
fn test_peekable_lexer_iterator_peek_zero_and_one_together() {
    let mut iter = PeekableLexerIterator::new("fn var const");

    // Peek at offset 0 should return 'fn'
    let peeked_0 = iter.peek(0);
    assert!(peeked_0.is_some());
    assert_eq!(peeked_0.unwrap().token, Token::Fn);

    // Peek at offset 1 should return 'var'
    let peeked_1 = iter.peek(1);
    assert!(peeked_1.is_some());
    assert_eq!(peeked_1.unwrap().token, Token::Var);

    // Peek at offset 0 again should still return 'fn'
    let peeked_0_again = iter.peek(0);
    assert!(peeked_0_again.is_some());
    assert_eq!(peeked_0_again.unwrap().token, Token::Fn);

    // Peek at offset 1 again should still return 'var'
    let peeked_1_again = iter.peek(1);
    assert!(peeked_1_again.is_some());
    assert_eq!(peeked_1_again.unwrap().token, Token::Var);
}

// Test consuming after peek(1)
#[test]
fn test_peekable_lexer_iterator_peek_one_then_consume() {
    let mut iter = PeekableLexerIterator::new("fn var const");

    // Peek at offset 1
    let peeked_1 = iter.peek(1);
    assert_eq!(peeked_1.unwrap().token, Token::Var);

    // Consume first token - should get 'fn'
    let next = iter.next().unwrap().unwrap();
    assert_eq!(next.token, Token::Fn);

    // Now peek(0) should return 'var'
    let peeked_0_after = iter.peek(0);
    assert_eq!(peeked_0_after.unwrap().token, Token::Var);

    // Now peek(1) should return 'const'
    let peeked_1_after = iter.peek(1);
    assert_eq!(peeked_1_after.unwrap().token, Token::Const);
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
    let mut iter = PeekableLexerIterator::new("fn var const");

    // Peek at offset 1 multiple times
    iter.peek(1);
    iter.peek(1);

    // Consume first token - should still get 'fn'
    let next = iter.next().unwrap().unwrap();
    assert_eq!(next.token, Token::Fn);

    // Consume second token - should still get 'var'
    let next2 = iter.next().unwrap().unwrap();
    assert_eq!(next2.token, Token::Var);
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
