/// Error type for action generation
#[derive(Debug)]
pub enum GenerateActionError {
    IoError(std::io::Error),
    JsonError(serde_json::Error),
    YamlError(serde_yaml::Error),
    ReqwestError(reqwest::Error),
    OperationNotFound(String),
    #[allow(dead_code)]
    TemplateError(String),
    SchemaError(String),
}

impl std::fmt::Display for GenerateActionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GenerateActionError::IoError(e) => write!(f, "IO error: {}", e),
            GenerateActionError::JsonError(e) => write!(f, "JSON error: {}", e),
            GenerateActionError::YamlError(e) => write!(f, "YAML error: {}", e),
            GenerateActionError::ReqwestError(e) => write!(f, "HTTP error: {}", e),
            GenerateActionError::OperationNotFound(op) => {
                write!(f, "Operation '{}' not found in schema", op)
            }
            GenerateActionError::TemplateError(msg) => write!(f, "Template error: {}", msg),
            GenerateActionError::SchemaError(msg) => write!(f, "Schema error: {}", msg),
        }
    }
}

impl std::error::Error for GenerateActionError {}

impl From<std::io::Error> for GenerateActionError {
    fn from(err: std::io::Error) -> Self {
        GenerateActionError::IoError(err)
    }
}

impl From<serde_json::Error> for GenerateActionError {
    fn from(err: serde_json::Error) -> Self {
        GenerateActionError::JsonError(err)
    }
}

impl From<serde_yaml::Error> for GenerateActionError {
    fn from(err: serde_yaml::Error) -> Self {
        GenerateActionError::YamlError(err)
    }
}

impl From<reqwest::Error> for GenerateActionError {
    fn from(err: reqwest::Error) -> Self {
        GenerateActionError::ReqwestError(err)
    }
}
