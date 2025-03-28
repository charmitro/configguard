use crate::validation::ValidationError;
use std::error::Error;
use std::fmt;
use std::path::PathBuf;

/// Result type for ConfigGuard operations
pub type ConfigGuardResult<T> = Result<T, ConfigGuardError>;

/// Custom error type for ConfigGuard
#[derive(Debug)]
pub enum ConfigGuardError {
    /// Error reading from a file
    FileRead {
        /// Path of the file
        path: String,
        /// Error message
        error: String,
    },

    /// Error writing to a file
    #[allow(dead_code)]
    FileWrite {
        /// Path of the file
        path: String,
        /// Error message
        error: String,
    },

    /// File not found
    FileNotFound {
        /// Path of the file
        path: PathBuf,
    },

    /// Error parsing YAML
    ParseYaml(String),

    /// Error parsing JSON
    ParseJson(String),

    /// Unsupported file format
    UnsupportedFormat {
        /// Path of the file
        path: String,
        /// File extension
        extension: String,
    },

    /// Error serializing to JSON
    Serialization(String),

    /// Error with I/O operations
    IO(String),

    /// Error with encoding/decoding
    Encoding(String),

    /// Validation failed
    Validation {
        /// Error message
        message: String,
        /// Path to the error location
        path: String,
        /// Expected value/type/constraint
        expected: String,
        /// Actual value
        actual: String,
    },

    /// Multiple validation errors
    ValidationErrors {
        /// Number of validation errors
        count: usize,
        /// First validation error (as a sample)
        source: Box<ConfigGuardError>,
    },

    /// All validation errors (for comprehensive error reporting)
    AllValidationErrors {
        /// All validation errors
        errors: Vec<ValidationError>,
    },

    /// Invalid pattern in schema
    Pattern(String),

    /// Command-line argument error
    Cli(String),

    /// Schema validation error
    Schema(String),

    #[allow(dead_code)]
    /// Internal error
    Internal(String),
}

impl fmt::Display for ConfigGuardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigGuardError::FileRead { path, error } => {
                write!(f, "Failed to read file '{}': {}", path, error)
            }
            ConfigGuardError::FileWrite { path, error } => {
                write!(f, "Failed to write to file '{}': {}", path, error)
            }
            ConfigGuardError::FileNotFound { path } => {
                write!(f, "File not found: {}", path.display())
            }
            ConfigGuardError::ParseYaml(msg) => {
                write!(f, "Failed to parse YAML: {}", msg)
            }
            ConfigGuardError::ParseJson(msg) => {
                write!(f, "Failed to parse JSON: {}", msg)
            }
            ConfigGuardError::UnsupportedFormat { path, extension } => {
                write!(
                    f,
                    "Unsupported file format for '{}' with extension '{}'",
                    path, extension
                )
            }
            ConfigGuardError::Serialization(msg) => {
                write!(f, "Serialization error: {}", msg)
            }
            ConfigGuardError::IO(msg) => {
                write!(f, "I/O error: {}", msg)
            }
            ConfigGuardError::Encoding(msg) => {
                write!(f, "Encoding error: {}", msg)
            }
            ConfigGuardError::Validation {
                message,
                path,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "Validation error at '{}': {} (expected: {}, found: {})",
                    path, message, expected, actual
                )
            }
            ConfigGuardError::ValidationErrors { count, source } => {
                write!(f, "{} validation errors (first error: {})", count, source)
            }
            ConfigGuardError::AllValidationErrors { errors } => {
                write!(f, "{} validation errors", errors.len())?;
                if !errors.is_empty() {
                    write!(
                        f,
                        " (first error: Validation error at '{}': {})",
                        errors[0].path, errors[0].message
                    )?;
                }
                Ok(())
            }
            ConfigGuardError::Pattern(msg) => {
                write!(f, "Invalid pattern: {}", msg)
            }
            ConfigGuardError::Cli(msg) => {
                write!(f, "CLI error: {}", msg)
            }
            ConfigGuardError::Schema(msg) => {
                write!(f, "Schema error: {}", msg)
            }
            ConfigGuardError::Internal(msg) => {
                write!(f, "Internal error: {}", msg)
            }
        }
    }
}

impl Error for ConfigGuardError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ConfigGuardError::ValidationErrors { source, .. } => Some(source),
            ConfigGuardError::AllValidationErrors { .. } => None,
            _ => None,
        }
    }
}

impl ConfigGuardError {
    /// Get the appropriate exit code for this error
    pub fn exit_code(&self) -> i32 {
        match self {
            ConfigGuardError::FileNotFound { .. } => 2,
            ConfigGuardError::FileRead { .. } | ConfigGuardError::FileWrite { .. } => 3,
            ConfigGuardError::ParseYaml(_) | ConfigGuardError::ParseJson(_) => 4,
            ConfigGuardError::UnsupportedFormat { .. } => 5,
            ConfigGuardError::Validation { .. }
            | ConfigGuardError::ValidationErrors { .. }
            | ConfigGuardError::AllValidationErrors { .. } => 10,
            ConfigGuardError::Schema(_) => 11,
            ConfigGuardError::Pattern(_) => 12,
            ConfigGuardError::Cli(_) => 20,
            ConfigGuardError::Serialization(_)
            | ConfigGuardError::Encoding(_)
            | ConfigGuardError::IO(_) => 30,
            ConfigGuardError::Internal(_) => 99,
        }
    }
}
