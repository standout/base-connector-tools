/// Error type for trigger generation
#[derive(Debug)]
pub enum GenerateTriggerError {
    IoError(std::io::Error),
    JsonError(serde_json::Error),
    YamlError(serde_yaml::Error),
    ReqwestError(reqwest::Error),
    OperationNotFound(String),
    TemplateError(String),
    SchemaError(String),
}

impl std::fmt::Display for GenerateTriggerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GenerateTriggerError::IoError(e) => write!(f, "IO error: {}", e),
            GenerateTriggerError::JsonError(e) => write!(f, "JSON error: {}", e),
            GenerateTriggerError::YamlError(e) => write!(f, "YAML error: {}", e),
            GenerateTriggerError::ReqwestError(e) => write!(f, "HTTP error: {}", e),
            GenerateTriggerError::OperationNotFound(op) => {
                write!(f, "Operation '{}' not found in schema", op)
            }
            GenerateTriggerError::TemplateError(msg) => write!(f, "Template error: {}", msg),
            GenerateTriggerError::SchemaError(msg) => write!(f, "Schema error: {}", msg),
        }
    }
}

impl std::error::Error for GenerateTriggerError {}

impl From<std::io::Error> for GenerateTriggerError {
    fn from(err: std::io::Error) -> Self {
        GenerateTriggerError::IoError(err)
    }
}

impl From<serde_json::Error> for GenerateTriggerError {
    fn from(err: serde_json::Error) -> Self {
        GenerateTriggerError::JsonError(err)
    }
}

impl From<serde_yaml::Error> for GenerateTriggerError {
    fn from(err: serde_yaml::Error) -> Self {
        GenerateTriggerError::YamlError(err)
    }
}

impl From<reqwest::Error> for GenerateTriggerError {
    fn from(err: reqwest::Error) -> Self {
        GenerateTriggerError::ReqwestError(err)
    }
}
