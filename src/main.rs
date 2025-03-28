use crate::error::ConfigGuardError;
use crate::reporting::ReportFormat;
use std::process;

mod cli;
mod config;
mod error;
mod reporting;
mod schema;
mod validation;

fn main() {
    // Parse command-line arguments
    let matches = cli::cli().get_matches();

    // Get output format
    let output_format = cli::get_output_format(&matches);

    // Execute command based on arguments
    let result = cli::run(&matches);

    // Handle result and exit
    match result {
        cli::RunResult::Success => {
            process::exit(0);
        }
        cli::RunResult::Failure(err) => {
            // If JSON format is requested, output errors in JSON
            if let Some(ReportFormat::Json) = output_format {
                // Handle different error types
                match &err {
                    // Direct access to all validation errors - preferred path
                    ConfigGuardError::AllValidationErrors { errors } => {
                        let json_errors: Vec<serde_json::Value> = errors
                            .iter()
                            .map(|e| {
                                serde_json::json!({
                                    "path": e.path,
                                    "message": e.message,
                                    "expected": e.expected,
                                    "actual": e.actual,
                                    "description": e.description
                                })
                            })
                            .collect();

                        let json_error = serde_json::json!({
                            "valid": false,
                            "error_count": errors.len(),
                            "errors": json_errors
                        });
                        println!("{}", serde_json::to_string_pretty(&json_error).unwrap());
                    }
                    // Other error types
                    _ => {
                        // Try to extract errors from other error types
                        match extract_validation_errors(&err) {
                            Some(errors) => {
                                let json_error = serde_json::json!({
                                    "valid": false,
                                    "error_count": errors.len(),
                                    "errors": errors
                                });
                                println!("{}", serde_json::to_string_pretty(&json_error).unwrap());
                            }
                            None => {
                                // Fallback for non-validation errors
                                let json_error = serde_json::json!({
                                    "valid": false,
                                    "error_count": 1,
                                    "errors": [{
                                        "message": err.to_string(),
                                        "path": get_error_path(&err),
                                        "exit_code": err.exit_code()
                                    }]
                                });
                                println!("{}", serde_json::to_string_pretty(&json_error).unwrap());
                            }
                        }
                    }
                }
            } else {
                // For text output, print the error message
                if let ConfigGuardError::AllValidationErrors { errors } = &err {
                    // Print all validation errors in text format
                    eprintln!("Error: {} validation errors found:", errors.len());
                    for (i, error) in errors.iter().enumerate() {
                        eprintln!(
                            "{}. Error at path '{}': {}",
                            i + 1,
                            error.path,
                            error.message
                        );
                        eprintln!("   Expected: {}", error.expected);
                        eprintln!("   Found: {}", error.actual);
                        if let Some(desc) = &error.description {
                            eprintln!("   Description: {}", desc);
                        }
                        if i < errors.len() - 1 {
                            eprintln!(); // Blank line between errors
                        }
                    }
                } else {
                    // For non-validation errors, just print the error
                    eprintln!("Error: {}", err);
                }
            }
            process::exit(err.exit_code());
        }
    }
}

/// Extract validation errors from the error chain
fn extract_validation_errors(err: &ConfigGuardError) -> Option<Vec<serde_json::Value>> {
    match err {
        // Handle the case where we have all validation errors - this is now the primary path
        ConfigGuardError::AllValidationErrors { errors } => {
            let json_errors: Vec<serde_json::Value> = errors
                .iter()
                .map(|e| {
                    serde_json::json!({
                        "path": e.path,
                        "message": e.message,
                        "expected": e.expected,
                        "actual": e.actual,
                        "description": e.description
                    })
                })
                .collect();

            if !json_errors.is_empty() {
                Some(json_errors)
            } else {
                None
            }
        }

        // Legacy support for ValidationErrors
        ConfigGuardError::ValidationErrors { count, source } => {
            // For now, we can only extract the first error
            let mut errors = Vec::new();

            // Add the first error
            if let ConfigGuardError::Validation {
                message,
                path,
                expected,
                actual,
            } = &**source
            {
                errors.push(serde_json::json!({
                    "path": path,
                    "message": message,
                    "expected": expected,
                    "actual": actual
                }));
            }

            // If we have count > 1, add a placeholder for additional errors
            if *count > 1 {
                errors.push(serde_json::json!({
                    "path": "",
                    "message": format!("and {} more validation errors", count - 1),
                    "expected": "",
                    "actual": ""
                }));
            }

            Some(errors)
        }

        // Single validation error
        ConfigGuardError::Validation {
            message,
            path,
            expected,
            actual,
        } => Some(vec![serde_json::json!({
            "path": path,
            "message": message,
            "expected": expected,
            "actual": actual
        })]),

        _ => None,
    }
}

/// Get path from error if available
fn get_error_path(err: &ConfigGuardError) -> String {
    match err {
        ConfigGuardError::Validation { path, .. } => path.clone(),
        ConfigGuardError::ValidationErrors { source, .. } => {
            if let ConfigGuardError::Validation { path, .. } = &**source {
                path.clone()
            } else {
                "".to_string()
            }
        }
        _ => "".to_string(),
    }
}
