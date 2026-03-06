use crate::ast::Span;

#[derive(Debug, Clone)]
pub struct AnalysisError {
    pub message: String,
    pub file: Option<String>,
    pub line: Option<usize>,
}

impl AnalysisError {
    pub fn new(message: &str) -> Self {
        AnalysisError {
            message: message.to_string(),
            file: None,
            line: None,
        }
    }

    pub fn new_with_span(message: &str, span: &Span) -> Self {
        AnalysisError {
            message: message.to_string(),
            file: None,
            line: Some(span.start),
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

    /// Get the full error message with location information
    pub fn format_message(&self) -> String {
        match (&self.file, self.line) {
            (Some(file), Some(line)) => format!("{}:{}", file, line),
            (Some(file), None) => file.to_string(),
            (None, Some(line)) => format!("line {}", line),
            (None, None) => String::new(),
        }
    }
}

impl std::fmt::Display for AnalysisError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let location = self.format_message();
        if location.is_empty() {
            write!(f, "{}", self.message)
        } else {
            write!(f, "{}: {}", location, self.message)
        }
    }
}

impl std::error::Error for AnalysisError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}

pub type AnalysisResult<T> = Result<T, AnalysisError>;
