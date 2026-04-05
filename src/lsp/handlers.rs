//! # LSP Request Handlers
//!
//! This module contains handlers for various LSP requests.

use crate::lsp::semantic_tokens::{compute_semantic_tokens, token_legend, tokens_to_response};
use crate::lsp::state::ServerState;
use lsp_server::ResponseError;
use url::Url;

/// Analyze a document and return diagnostics
pub fn analyze_document(content: &str) -> Vec<serde_json::Value> {
    let mut diagnostics = Vec::new();

    // Try to parse the document
    match crate::parser::parse(content) {
        Ok(mut program) => {
            // Run semantic analysis
            let mut analyzer = crate::sema::SemanticAnalyzer::new();
            if let Err(error) = analyzer.analyze(&mut program) {
                // Convert analysis error to LSP diagnostic
                let message = format!("{:?}", error);

                diagnostics.push(serde_json::json!({
                    "severity": 1, // Error
                    "range": {
                        "start": { "line": 0, "character": 0 },
                        "end": { "line": 0, "character": 1 }
                    },
                    "message": message,
                    "source": "lang"
                }));
            }
        }
        Err(error) => {
            // Convert parse error to LSP diagnostic
            diagnostics.push(serde_json::json!({
                "severity": 1, // Error
                "range": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 0, "character": 1 }
                },
                "message": format!("Parse error: {:?}", error),
                "source": "lang"
            }));
        }
    }

    diagnostics
}

/// Handle the initialize request
pub fn handle_initialize(
    _params: serde_json::Value,
    state: &ServerState,
) -> Result<serde_json::Value, ResponseError> {
    // Get root path from params
    if let Some(root_path) = _params.get("rootUri").and_then(|v| v.as_str()) {
        let path = if root_path.starts_with("file://") {
            Some(std::path::PathBuf::from(&root_path[7..]))
        } else {
            Some(std::path::PathBuf::from(root_path))
        };
        *state.root_path.write() = path;
    }

    // Return server capabilities
    let capabilities = serde_json::json!({
        "positionEncoding": "utf-16",
        "textDocumentSync": {
            "openClose": true,
            "change": 1, // Full text sync
            "willSave": false,
            "willSaveWaitUntil": false,
            "save": {
                "includeText": false
            }
        },
        "semanticTokensProvider": {
            "legend": {
                "tokenTypes": token_legend().0,
                "tokenModifiers": token_legend().1
            },
            "range": false,
            "full": true
        },
        "completionProvider": {
            "triggerCharacters": [".", ":"],
            "resolveProvider": false
        },
        "hoverProvider": true,
        "definitionProvider": true,
        "typeDefinitionProvider": true,
        "referencesProvider": true,
        "documentFormattingProvider": false,
        "diagnosticProvider": {
            "identifier": "lang",
            "interFileDependencies": false,
            "workspaceDiagnostics": false
        }
    });

    Ok(capabilities)
}

/// Handle the initialized notification
pub fn handle_initialized(_params: serde_json::Value) {
    // Nothing to do here
}

/// Handle the shutdown request
pub fn handle_shutdown(_params: serde_json::Value) -> Result<serde_json::Value, ResponseError> {
    Ok(serde_json::Value::Null)
}

/// Handle exit notification
pub fn handle_exit(_params: serde_json::Value) -> i32 {
    0
}

/// Handle textDocument/didOpen
pub fn handle_text_document_did_open(
    params: serde_json::Value,
    state: &ServerState,
) -> Result<(), ResponseError> {
    let text_document = params.get("textDocument").ok_or_else(|| ResponseError {
        code: -32602,
        message: "Missing textDocument parameter".to_string(),
        data: None,
    })?;

    let uri = text_document
        .get("uri")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ResponseError {
            code: -32602,
            message: "Missing uri parameter".to_string(),
            data: None,
        })?;

    let text = text_document
        .get("text")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let version = text_document
        .get("version")
        .and_then(|v| v.as_i64())
        .unwrap_or(1) as i32;

    let uri = Url::parse(uri).map_err(|e| ResponseError {
        code: -32602,
        message: format!("Invalid URI: {}", e),
        data: None,
    })?;

    state.update_document(uri, text.to_string(), version);

    Ok(())
}

/// Handle textDocument/didChange
pub fn handle_text_document_did_change(
    params: serde_json::Value,
    state: &ServerState,
) -> Result<(), ResponseError> {
    let text_document = params.get("textDocument").ok_or_else(|| ResponseError {
        code: -32602,
        message: "Missing textDocument parameter".to_string(),
        data: None,
    })?;

    let uri = text_document
        .get("uri")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ResponseError {
            code: -32602,
            message: "Missing uri parameter".to_string(),
            data: None,
        })?;

    let version = text_document
        .get("version")
        .and_then(|v| v.as_i64())
        .unwrap_or(1) as i32;

    let content_changes = params.get("contentChanges").and_then(|v| v.as_array());
    let text = content_changes
        .and_then(|arr| arr.last())
        .and_then(|v| v.get("text"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let uri = Url::parse(uri).map_err(|e| ResponseError {
        code: -32602,
        message: format!("Invalid URI: {}", e),
        data: None,
    })?;

    state.update_document(uri, text.to_string(), version);

    Ok(())
}

/// Handle textDocument/didClose
pub fn handle_text_document_did_close(
    params: serde_json::Value,
    state: &ServerState,
) -> Result<(), ResponseError> {
    let text_document = params.get("textDocument").ok_or_else(|| ResponseError {
        code: -32602,
        message: "Missing textDocument parameter".to_string(),
        data: None,
    })?;

    let uri = text_document
        .get("uri")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ResponseError {
            code: -32602,
            message: "Missing uri parameter".to_string(),
            data: None,
        })?;

    let uri = Url::parse(uri).map_err(|e| ResponseError {
        code: -32602,
        message: format!("Invalid URI: {}", e),
        data: None,
    })?;

    state.remove_document(&uri);

    Ok(())
}

/// Handle textDocument/semanticTokens/full
pub fn handle_semantic_tokens_full(
    params: serde_json::Value,
    state: &ServerState,
) -> Result<serde_json::Value, ResponseError> {
    let text_document = params.get("textDocument").ok_or_else(|| ResponseError {
        code: -32602,
        message: "Missing textDocument parameter".to_string(),
        data: None,
    })?;

    let uri = text_document
        .get("uri")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ResponseError {
            code: -32602,
            message: "Missing uri parameter".to_string(),
            data: None,
        })?;

    let uri = Url::parse(uri).map_err(|e| ResponseError {
        code: -32602,
        message: format!("Invalid URI: {}", e),
        data: None,
    })?;

    let document = state.get_document(&uri).ok_or_else(|| ResponseError {
        code: -32602,
        message: "Document not found".to_string(),
        data: None,
    })?;

    let text = document.text.read().clone();
    let tokens = compute_semantic_tokens(&text);
    let response = tokens_to_response(tokens);

    Ok(response)
}

/// Handle textDocument/completion
pub fn handle_completion(
    params: serde_json::Value,
    state: &ServerState,
) -> Result<serde_json::Value, ResponseError> {
    let text_document = params.get("textDocument").ok_or_else(|| ResponseError {
        code: -32602,
        message: "Missing textDocument parameter".to_string(),
        data: None,
    })?;

    let uri = text_document
        .get("uri")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ResponseError {
            code: -32602,
            message: "Missing uri parameter".to_string(),
            data: None,
        })?;

    let uri = Url::parse(uri).map_err(|e| ResponseError {
        code: -32602,
        message: format!("Invalid URI: {}", e),
        data: None,
    })?;

    let _position = params.get("position").ok_or_else(|| ResponseError {
        code: -32602,
        message: "Missing position parameter".to_string(),
        data: None,
    })?;

    // Get the document
    let document = match state.get_document(&uri) {
        Some(doc) => doc,
        None => {
            return Ok(serde_json::json!([]));
        }
    };

    let _text = document.text.read().clone();

    // Provide basic completions
    let completions = get_completions();

    Ok(serde_json::to_value(completions).unwrap_or(serde_json::json!([])))
}

/// Get completion items based on context
fn get_completions() -> Vec<serde_json::Value> {
    let mut completions = Vec::new();

    // Keywords
    let keywords = vec![
        "fn",
        "pub",
        "var",
        "const",
        "return",
        "import",
        "struct",
        "enum",
        "if",
        "else",
        "for",
        "switch",
        "true",
        "false",
        "null",
        "self",
        "extern",
        "defer",
        "defer!",
        "error",
        "try",
        "catch",
        "break",
        "continue",
        "inline",
        "varargs",
        "impl",
        "interface",
    ];

    for kw in keywords {
        completions.push(serde_json::json!({
            "label": kw,
            "kind": 14, // Keyword
            "insertText": kw,
        }));
    }

    // Built-in types
    let types = vec![
        "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "f32", "f64", "bool", "void",
        "rawptr",
    ];

    for ty in types {
        completions.push(serde_json::json!({
            "label": ty,
            "kind": 7, // Type
            "insertText": ty,
        }));
    }

    completions
}

/// Handle textDocument/hover
pub fn handle_hover(
    params: serde_json::Value,
    state: &ServerState,
) -> Result<serde_json::Value, ResponseError> {
    let text_document = params.get("textDocument").ok_or_else(|| ResponseError {
        code: -32602,
        message: "Missing textDocument parameter".to_string(),
        data: None,
    })?;

    let uri = text_document
        .get("uri")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ResponseError {
            code: -32602,
            message: "Missing uri parameter".to_string(),
            data: None,
        })?;

    let uri = Url::parse(uri).map_err(|e| ResponseError {
        code: -32602,
        message: format!("Invalid URI: {}", e),
        data: None,
    })?;

    // For now, return empty hover
    let document = match state.get_document(&uri) {
        Some(doc) => doc,
        None => {
            return Ok(serde_json::Value::Null);
        }
    };

    let _text = document.text.read().clone();

    Ok(serde_json::Value::Null)
}

/// Handle textDocument/definition
pub fn handle_definition(
    params: serde_json::Value,
    state: &ServerState,
) -> Result<serde_json::Value, ResponseError> {
    let text_document = params.get("textDocument").ok_or_else(|| ResponseError {
        code: -32602,
        message: "Missing textDocument parameter".to_string(),
        data: None,
    })?;

    let uri = text_document
        .get("uri")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ResponseError {
            code: -32602,
            message: "Missing uri parameter".to_string(),
            data: None,
        })?;

    let uri = Url::parse(uri).map_err(|e| ResponseError {
        code: -32602,
        message: format!("Invalid URI: {}", e),
        data: None,
    })?;

    let position = params.get("position").ok_or_else(|| ResponseError {
        code: -32602,
        message: "Missing position parameter".to_string(),
        data: None,
    })?;

    let document = match state.get_document(&uri) {
        Some(doc) => doc,
        None => {
            return Ok(serde_json::Value::Null);
        }
    };

    let text = document.text.read().clone();
    let line = position.get("line").and_then(|v| v.as_i64()).unwrap_or(0) as usize;
    let character = position
        .get("character")
        .and_then(|v| v.as_i64())
        .unwrap_or(0) as usize;

    let offset = position_to_offset(&text, line, character);
    if let Some(offset) = offset {
        if let Some(def_location) = find_definition(&text, offset, &uri) {
            return Ok(serde_json::to_value(def_location).unwrap_or(serde_json::Value::Null));
        }
    }

    Ok(serde_json::Value::Null)
}

/// Handle textDocument/typeDefinition
pub fn handle_type_definition(
    params: serde_json::Value,
    state: &ServerState,
) -> Result<serde_json::Value, ResponseError> {
    let text_document = params.get("textDocument").ok_or_else(|| ResponseError {
        code: -32602,
        message: "Missing textDocument parameter".to_string(),
        data: None,
    })?;

    let uri = text_document
        .get("uri")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ResponseError {
            code: -32602,
            message: "Missing uri parameter".to_string(),
            data: None,
        })?;

    let uri = Url::parse(uri).map_err(|e| ResponseError {
        code: -32602,
        message: format!("Invalid URI: {}", e),
        data: None,
    })?;

    let position = params.get("position").ok_or_else(|| ResponseError {
        code: -32602,
        message: "Missing position parameter".to_string(),
        data: None,
    })?;

    let document = match state.get_document(&uri) {
        Some(doc) => doc,
        None => {
            return Ok(serde_json::Value::Null);
        }
    };

    let text = document.text.read().clone();
    let line = position.get("line").and_then(|v| v.as_i64()).unwrap_or(0) as usize;
    let character = position
        .get("character")
        .and_then(|v| v.as_i64())
        .unwrap_or(0) as usize;

    let offset = position_to_offset(&text, line, character);
    if let Some(offset) = offset {
        if let Some(def_location) = find_type_definition(&text, offset, &uri) {
            return Ok(serde_json::to_value(def_location).unwrap_or(serde_json::Value::Null));
        }
    }

    Ok(serde_json::Value::Null)
}

fn position_to_offset(text: &str, line: usize, character: usize) -> Option<usize> {
    let mut current_line = 0;
    let mut current_offset = 0;

    for ch in text.chars() {
        if current_line == line {
            if character <= ch.len_utf8() || (ch.is_ascii() && character <= 1) {
                return Some(current_offset);
            }
            if ch.is_ascii() {
                if character == 1 {
                    return Some(current_offset);
                }
            }
        }
        if ch == '\n' {
            current_line += 1;
            if current_line > line {
                return Some(current_offset);
            }
        }
        current_offset += ch.len_utf8();
    }

    if current_line == line {
        Some(current_offset)
    } else {
        None
    }
}

fn find_identifier_at_offset(text: &str, offset: usize) -> Option<(String, usize, usize)> {
    let chars: Vec<char> = text.chars().collect();
    if offset >= chars.len() {
        return None;
    }

    let mut start = offset;
    while start > 0 {
        let ch = chars[start - 1];
        if ch.is_alphanumeric() || ch == '_' {
            start -= 1;
        } else {
            break;
        }
    }

    let mut end = offset;
    while end < chars.len() {
        let ch = chars[end];
        if ch.is_alphanumeric() || ch == '_' {
            end += 1;
        } else {
            break;
        }
    }

    if end > start {
        let name: String = chars[start..end].iter().collect();
        Some((name, start, end))
    } else {
        None
    }
}

fn find_definition(text: &str, offset: usize, uri: &Url) -> Option<serde_json::Value> {
    let (name, start, end) = find_identifier_at_offset(text, offset)?;

    if name.is_empty() {
        return None;
    }

    match crate::parser::parse(text) {
        Ok(mut program) => {
            let mut analyzer = crate::sema::SemanticAnalyzer::new();
            if let Err(_) = analyzer.analyze(&mut program) {
                return None;
            }

            for f in &program.functions {
                if f.name == name {
                    let (line, char) = offset_to_position(text, f.span.start);
                    return Some(serde_json::json!({
                        "uri": uri.to_string(),
                        "range": {
                            "start": { "line": line, "character": char },
                            "end": { "line": line, "character": char + f.name.len() }
                        }
                    }));
                }
            }

            for s in &program.structs {
                if s.name == name {
                    let (line, char) = offset_to_position(text, s.span.start);
                    return Some(serde_json::json!({
                        "uri": uri.to_string(),
                        "range": {
                            "start": { "line": line, "character": char },
                            "end": { "line": line, "character": char + s.name.len() }
                        }
                    }));
                }
            }

            for e in &program.enums {
                if e.name == name {
                    let (line, char) = offset_to_position(text, e.span.start);
                    return Some(serde_json::json!({
                        "uri": uri.to_string(),
                        "range": {
                            "start": { "line": line, "character": char },
                            "end": { "line": line, "character": char + e.name.len() }
                        }
                    }));
                }
            }
        }
        Err(_) => {}
    }

    None
}

fn find_type_definition(text: &str, offset: usize, uri: &Url) -> Option<serde_json::Value> {
    let (name, start, end) = find_identifier_at_offset(text, offset)?;

    if name.is_empty() {
        return None;
    }

    match crate::parser::parse(text) {
        Ok(mut program) => {
            let mut analyzer = crate::sema::SemanticAnalyzer::new();
            if let Err(_) = analyzer.analyze(&mut program) {
                return None;
            }

            if let Some(symbol) = analyzer.symbol_table.resolve(&name) {
                let ty = symbol.ty.clone();
                return find_type_location(&ty, text, uri);
            }

            for f in &program.functions {
                for param in &f.params {
                    if param.name == name {
                        return find_type_location(&param.ty, text, uri);
                    }
                }
            }

            for s in &program.structs {
                for field in &s.fields {
                    if field.name == name {
                        return find_type_location(&field.ty, text, uri);
                    }
                }
            }
        }
        Err(_) => {}
    }

    None
}

fn find_type_location(ty: &crate::ast::Type, text: &str, uri: &Url) -> Option<serde_json::Value> {
    match ty {
        crate::ast::Type::Custom { name, .. } => match crate::parser::parse(text) {
            Ok(program) => {
                for s in &program.structs {
                    if s.name == *name {
                        let (line, char) = offset_to_position(text, s.span.start);
                        return Some(serde_json::json!({
                            "uri": uri.to_string(),
                            "range": {
                                "start": { "line": line, "character": char },
                                "end": { "line": line, "character": char + s.name.len() }
                            }
                        }));
                    }
                }
                for e in &program.enums {
                    if e.name == *name {
                        let (line, char) = offset_to_position(text, e.span.start);
                        return Some(serde_json::json!({
                            "uri": uri.to_string(),
                            "range": {
                                "start": { "line": line, "character": char },
                                "end": { "line": line, "character": char + e.name.len() }
                            }
                        }));
                    }
                }
            }
            Err(_) => {}
        },
        _ => {}
    }
    None
}

fn offset_to_position(text: &str, offset: usize) -> (usize, usize) {
    let mut line = 0;
    let mut line_start = 0;
    let chars: Vec<char> = text.chars().collect();

    for (i, ch) in chars.iter().enumerate() {
        if i >= offset {
            break;
        }
        if *ch == '\n' {
            line += 1;
            line_start = i + 1;
        }
    }

    let character = offset - line_start;
    (line, character)
}
