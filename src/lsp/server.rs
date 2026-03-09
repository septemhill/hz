//! # Lang Language Server
//!
//! This module implements the Language Server Protocol for the Lang language.

use crate::lsp::handlers::*;
use crate::lsp::state::ServerState;
use lsp_server::{Connection, Message, Request, Response};
use url::Url;

/// Run the LSP server
pub fn run_lsp_server() {
    // Create the connection
    let (connection, io_threads) = Connection::stdio();

    // Create the server state
    let state = ServerState::new();

    // Wait for the initialize request
    let _server_capabilities = loop {
        match connection.receiver.recv() {
            Ok(Message::Request(request)) => {
                if request.method == "initialize" {
                    let params = request.params;
                    match handle_initialize(params, &state) {
                        Ok(capabilities) => {
                            let resp = Response {
                                id: request.id,
                                result: Some(capabilities.clone()),
                                error: None,
                            };
                            let _ = connection.sender.send(Message::Response(resp));
                            break capabilities;
                        }
                        Err(e) => {
                            let resp = Response {
                                id: request.id,
                                result: None,
                                error: Some(e),
                            };
                            let _ = connection.sender.send(Message::Response(resp));
                            return;
                        }
                    }
                }
            }
            Ok(Message::Notification(_)) => continue,
            Ok(Message::Response(_)) => continue,
            Err(_) => return,
        }
    };

    // Send initialized notification
    let _ = connection
        .sender
        .send(Message::Notification(lsp_server::Notification {
            method: "initialized".to_string(),
            params: serde_json::Value::Object(serde_json::Map::new()),
        }));

    // Main loop
    loop {
        match connection.receiver.recv() {
            Ok(Message::Request(request)) => {
                handle_request(request, &state, &connection);
            }
            Ok(Message::Notification(notification)) => {
                handle_notification(notification, &state, &connection);
            }
            Ok(Message::Response(_)) => {}
            Err(_) => break,
        }
    }

    // Wait for threads to finish
    let _ = io_threads.join();
}

/// Handle an incoming request
fn handle_request(request: Request, state: &ServerState, connection: &Connection) {
    let response = match request.method.as_str() {
        "shutdown" => match handle_shutdown(request.params) {
            Ok(result) => Response {
                id: request.id,
                result: Some(result),
                error: None,
            },
            Err(e) => Response {
                id: request.id,
                result: None,
                error: Some(e),
            },
        },
        "textDocument/semanticTokens/full" => {
            match handle_semantic_tokens_full(request.params, state) {
                Ok(result) => Response {
                    id: request.id,
                    result: Some(result),
                    error: None,
                },
                Err(e) => Response {
                    id: request.id,
                    result: None,
                    error: Some(e),
                },
            }
        }
        "textDocument/completion" => match handle_completion(request.params, state) {
            Ok(result) => Response {
                id: request.id,
                result: Some(result),
                error: None,
            },
            Err(e) => Response {
                id: request.id,
                result: None,
                error: Some(e),
            },
        },
        "textDocument/hover" => match handle_hover(request.params, state) {
            Ok(result) => Response {
                id: request.id,
                result: Some(result),
                error: None,
            },
            Err(e) => Response {
                id: request.id,
                result: None,
                error: Some(e),
            },
        },
        // Echo other requests back (not supported yet)
        _ => Response {
            id: request.id,
            result: None,
            error: Some(lsp_server::ResponseError {
                code: -32601,
                message: format!("Method not found: {}", request.method),
                data: None,
            }),
        },
    };

    let _ = connection.sender.send(Message::Response(response));
}

/// Handle an incoming notification
fn handle_notification(
    notification: lsp_server::Notification,
    state: &ServerState,
    connection: &Connection,
) {
    match notification.method.as_str() {
        "exit" => {
            std::process::exit(0);
        }
        "textDocument/didOpen" => {
            // Extract URI before consuming params
            let uri = get_uri_from_did_open(&notification.params);
            let _ = handle_text_document_did_open(notification.params, state);
            // Publish diagnostics
            if let Some(uri) = uri {
                publish_diagnostics(&uri, state, connection);
            }
        }
        "textDocument/didChange" => {
            // Extract URI before consuming params
            let uri = get_uri_from_did_change(&notification.params);
            let _ = handle_text_document_did_change(notification.params, state);
            // Publish diagnostics
            if let Some(uri) = uri {
                publish_diagnostics(&uri, state, connection);
            }
        }
        "textDocument/didClose" => {
            let _ = handle_text_document_did_close(notification.params, state);
        }
        _ => {}
    }
}

/// Get URI from didOpen notification params
fn get_uri_from_did_open(params: &serde_json::Value) -> Option<Url> {
    let text_document = params.get("textDocument")?;
    let uri = text_document.get("uri")?.as_str()?;
    Url::parse(uri).ok()
}

/// Get URI from didChange notification params
fn get_uri_from_did_change(params: &serde_json::Value) -> Option<Url> {
    let text_document = params.get("textDocument")?;
    let uri = text_document.get("uri")?.as_str()?;
    Url::parse(uri).ok()
}

/// Publish diagnostics for a document
fn publish_diagnostics(uri: &Url, state: &ServerState, connection: &Connection) {
    if let Some(document) = state.get_document(uri) {
        let text = document.text.read().clone();
        let diagnostics = analyze_document(&text);

        let notification = lsp_server::Notification {
            method: "textDocument/publishDiagnostics".to_string(),
            params: serde_json::json!({
                "uri": uri.to_string(),
                "diagnostics": diagnostics
            }),
        };

        let _ = connection.sender.send(Message::Notification(notification));
    }
}
