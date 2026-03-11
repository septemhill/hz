use crate::ast::Span;

#[derive(Debug, Clone)]
pub struct AnalysisError {
    pub message: String,
    pub file: Option<String>,
    pub line: Option<usize>,
    pub module: Option<String>,
}

impl AnalysisError {
    pub fn new(message: &str) -> Self {
        AnalysisError {
            message: message.to_string(),
            file: None,
            line: None,
            module: None,
        }
    }

    pub fn new_with_span(message: &str, span: &Span) -> Self {
        AnalysisError {
            message: message.to_string(),
            file: None,
            line: Some(span.start),
            module: None,
        }
    }

    pub fn with_file(mut self, file: &str) -> Self {
        self.file = Some(file.to_string());
        self
    }

    pub fn with_line(mut self, line: usize) -> Self {
        self.line = Some(line);
        self
    }

    pub fn with_module(mut self, module: &str) -> Self {
        self.module = Some(module.to_string());
        self
    }

    /// Get the full error message with location information
    pub fn format_message(&self) -> String {
        match (&self.file, self.line) {
            (Some(file), Some(line)) => format!("{}:{}", file, line),
            (Some(file), None) => file.to_string(),
            (None, Some(line)) => format!("line {}", line),
            (None, None) => String::new(),
        }
    }

    /// Get the module prefix for error message
    fn format_module_prefix(&self) -> String {
        match &self.module {
            Some(module) => format!("[sema/{}]", module),
            None => "[sema]".to_string(),
        }
    }
}

impl std::fmt::Display for AnalysisError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let module_prefix = self.format_module_prefix();
        let location = self.format_message();
        if location.is_empty() {
            write!(f, "{}: {}", module_prefix, self.message)
        } else {
            write!(f, "{}: {}: {}", module_prefix, location, self.message)
        }
    }
}

impl std::error::Error for AnalysisError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}

pub type AnalysisResult<T> = Result<T, AnalysisError>;
