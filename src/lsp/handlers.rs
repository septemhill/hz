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
        "fn", "pub", "var", "const", "return", "import", "struct", "enum", "if", "else", "for",
        "switch", "true", "false", "null", "self", "external", "cdecl", "defer", "defer!", "error",
        "try", "catch", "break",
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
