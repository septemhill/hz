//! # LSP Server State
//!
//! Manages the state of the language server including opened documents.

use parking_lot::RwLock;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use url::Url;

/// Represents an opened document in the language server
pub struct Document {
    /// The URI of the document
    pub uri: Url,
    /// The content of the document
    pub text: Arc<RwLock<String>>,
    /// The version of the document
    pub version: i32,
}

impl Document {
    /// Create a new document
    pub fn new(uri: Url, text: String, version: i32) -> Self {
        Self {
            uri,
            text: Arc::new(RwLock::new(text)),
            version,
        }
    }
}

impl Clone for Document {
    fn clone(&self) -> Self {
        Self {
            uri: self.uri.clone(),
            text: Arc::clone(&self.text),
            version: self.version,
        }
    }
}

/// The global state of the language server
pub struct ServerState {
    /// Map of document URIs to documents
    pub documents: RwLock<HashMap<Url, Document>>,
    /// The root path of the workspace
    pub root_path: RwLock<Option<PathBuf>>,
}

impl ServerState {
    /// Create a new server state
    pub fn new() -> Self {
        Self {
            documents: RwLock::new(HashMap::new()),
            root_path: RwLock::new(None),
        }
    }

    /// Get a document by URI
    pub fn get_document(&self, uri: &Url) -> Option<Document> {
        self.documents.read().get(uri).cloned()
    }

    /// Add or update a document
    pub fn update_document(&self, uri: Url, text: String, version: i32) {
        let mut docs = self.documents.write();
        if let Some(doc) = docs.get_mut(&uri) {
            *doc.text.write() = text;
            doc.version = version;
        } else {
            docs.insert(uri.clone(), Document::new(uri, text, version));
        }
    }

    /// Remove a document
    pub fn remove_document(&self, uri: &Url) {
        self.documents.write().remove(uri);
    }
}

impl Default for ServerState {
    fn default() -> Self {
        Self::new()
    }
}
