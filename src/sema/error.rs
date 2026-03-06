#[derive(Debug, Clone)]
pub struct AnalysisError {
    pub message: String,
}

impl AnalysisError {
    pub fn new(message: &str) -> Self {
        AnalysisError {
            message: message.to_string(),
        }
    }
}

pub type AnalysisResult<T> = Result<T, AnalysisError>;
