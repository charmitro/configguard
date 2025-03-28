use crate::error::{ConfigGuardError, ConfigGuardResult};
use crate::validation::ValidationResult;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::io::Write;

/// Report format type
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ReportFormat {
    /// Human-readable text format
    Text,
    /// JSON format for machine consumption
    Json,
}

impl fmt::Display for ReportFormat {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ReportFormat::Text => write!(f, "text"),
            ReportFormat::Json => write!(f, "json"),
        }
    }
}

/// Format validation results as a report
pub fn format_validation_result(
    result: &ValidationResult,
    format: &ReportFormat,
) -> ConfigGuardResult<String> {
    match format {
        ReportFormat::Text => format_text_report(result),
        ReportFormat::Json => format_json_report(result),
    }
}

/// Format validation results as a text report
fn format_text_report(result: &ValidationResult) -> ConfigGuardResult<String> {
    let mut output = Vec::new();

    match result {
        ValidationResult::Valid => {
            writeln!(output, "Configuration validation passed.")
                .map_err(|e| ConfigGuardError::IO(e.to_string()))?;
        }
        ValidationResult::Invalid(errors) => {
            writeln!(
                output,
                "Configuration validation failed with {} errors:",
                errors.len()
            )
            .map_err(|e| ConfigGuardError::IO(e.to_string()))?;

            for (i, error) in errors.iter().enumerate() {
                if let Some(line) = error.line {
                    writeln!(
                        output,
                        "{}. Error at path '{}' (line {}): {}",
                        i + 1,
                        error.path,
                        line,
                        error.message
                    )
                    .map_err(|e| ConfigGuardError::IO(e.to_string()))?;
                } else {
                    writeln!(
                        output,
                        "{}. Error at path '{}': {}",
                        i + 1,
                        error.path,
                        error.message
                    )
                    .map_err(|e| ConfigGuardError::IO(e.to_string()))?;
                }

                // Include field description if available
                if let Some(description) = &error.description {
                    writeln!(output, "   Field description: {}", description)
                        .map_err(|e| ConfigGuardError::IO(e.to_string()))?;
                }

                writeln!(output, "   Expected: {}", error.expected)
                    .map_err(|e| ConfigGuardError::IO(e.to_string()))?;

                writeln!(output, "   Found: {}", error.actual)
                    .map_err(|e| ConfigGuardError::IO(e.to_string()))?;

                // Add a blank line between errors for readability
                if i < errors.len() - 1 {
                    writeln!(output).map_err(|e| ConfigGuardError::IO(e.to_string()))?;
                }
            }
        }
    }

    String::from_utf8(output).map_err(|e| ConfigGuardError::Encoding(e.to_string()))
}

/// JSON report structure
#[derive(Serialize, Deserialize)]
struct JsonReport {
    valid: bool,
    error_count: usize,
    errors: Vec<JsonValidationError>,
}

/// JSON validation error structure
#[derive(Serialize, Deserialize)]
struct JsonValidationError {
    path: String,
    message: String,
    expected: String,
    actual: String,
    description: Option<String>,
    line: Option<usize>,
}

/// Format validation results as a JSON report
fn format_json_report(result: &ValidationResult) -> ConfigGuardResult<String> {
    let report = match result {
        ValidationResult::Valid => JsonReport {
            valid: true,
            error_count: 0,
            errors: Vec::new(),
        },
        ValidationResult::Invalid(errors) => {
            let json_errors = errors
                .iter()
                .map(|e| JsonValidationError {
                    path: e.path.clone(),
                    message: e.message.clone(),
                    expected: e.expected.clone(),
                    actual: e.actual.clone(),
                    description: e.description.clone(),
                    line: e.line,
                })
                .collect();

            JsonReport {
                valid: false,
                error_count: errors.len(),
                errors: json_errors,
            }
        }
    };

    serde_json::to_string_pretty(&report).map_err(|e| {
        ConfigGuardError::Serialization(format!("Failed to serialize JSON report: {}", e))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validation::ValidationError;

    #[test]
    fn test_text_report_valid() {
        let result = ValidationResult::Valid;
        let report = format_text_report(&result).unwrap();

        assert!(report.contains("Configuration validation passed"));
    }

    #[test]
    fn test_text_report_invalid() {
        let errors = vec![
            ValidationError {
                path: ".metadata.name".to_string(),
                message: "Required key missing".to_string(),
                expected: "Key to be present".to_string(),
                actual: "Key is absent".to_string(),
                description: Some("The name of the resource".to_string()),
                line: None,
            },
            ValidationError {
                path: ".spec.containers".to_string(),
                message: "List too short".to_string(),
                expected: "At least 1 items".to_string(),
                actual: "0 items".to_string(),
                description: None,
                line: None,
            },
        ];

        let result = ValidationResult::Invalid(errors);
        let report = format_text_report(&result).unwrap();

        assert!(report.contains("Configuration validation failed with 2 errors"));
        assert!(report.contains("Error at path '.metadata.name'"));
        assert!(report.contains("Required key missing"));
        assert!(report.contains("Field description: The name of the resource"));
        assert!(report.contains("Error at path '.spec.containers'"));
        assert!(report.contains("List too short"));
    }

    #[test]
    fn test_json_report_valid() {
        let result = ValidationResult::Valid;
        let report = format_json_report(&result).unwrap();

        let parsed: JsonReport = serde_json::from_str(&report).unwrap();
        assert!(parsed.valid);
        assert_eq!(parsed.error_count, 0);
        assert!(parsed.errors.is_empty());
    }

    #[test]
    fn test_json_report_invalid() {
        let errors = vec![ValidationError {
            path: ".metadata.name".to_string(),
            message: "Required key missing".to_string(),
            expected: "Key to be present".to_string(),
            actual: "Key is absent".to_string(),
            description: Some("The name of the resource".to_string()),
            line: None,
        }];

        let result = ValidationResult::Invalid(errors);
        let report = format_json_report(&result).unwrap();

        let parsed: JsonReport = serde_json::from_str(&report).unwrap();
        assert!(!parsed.valid);
        assert_eq!(parsed.error_count, 1);
        assert_eq!(parsed.errors.len(), 1);
        assert_eq!(parsed.errors[0].path, ".metadata.name");
        assert_eq!(parsed.errors[0].message, "Required key missing");
        assert_eq!(
            parsed.errors[0].description,
            Some("The name of the resource".to_string())
        );
    }
}
