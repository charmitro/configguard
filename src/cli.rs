use crate::config::Config;
use crate::error::{ConfigGuardError, ConfigGuardResult};
use crate::reporting::{format_validation_result, ReportFormat};
use crate::schema::Schema;
use crate::validation::{validate, ValidationResult};
use clap::{Arg, ArgAction, Command};
use std::fs;
use std::path::{Path, PathBuf};

/// Result of the CLI command execution
#[derive(Debug)]
pub enum RunResult {
    /// Successful execution
    Success,

    /// Command execution failed
    Failure(ConfigGuardError),
}

/// Create the command-line interface definition
pub fn cli() -> Command {
    Command::new("configguard")
        .about("Configuration validation tool")
        .version("0.1.0")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(
            Command::new("validate")
                .about("Validate a configuration against a schema")
                .arg(
                    Arg::new("config")
                        .help("Path to the configuration file(s) to validate")
                        .required(true)
                        .num_args(1..),
                )
                .arg(
                    Arg::new("schema")
                        .short('s')
                        .long("schema")
                        .help("Path to the schema file")
                        .required(true)
                        .num_args(1),
                )
                .arg(
                    Arg::new("format")
                        .short('f')
                        .long("format")
                        .help("Output format (text or json)")
                        .default_value("text")
                        .value_parser(["text", "json"]),
                )
                .arg(
                    Arg::new("strict")
                        .long("strict")
                        .help("Enable strict mode (reject unknown keys)")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("directory")
                        .short('d')
                        .long("directory")
                        .help("Validate all compatible files in given directories")
                        .action(ArgAction::SetTrue),
                ),
        )
}

/// Run the CLI command
pub fn run(matches: &clap::ArgMatches) -> RunResult {
    match matches.subcommand() {
        Some(("validate", sub_matches)) => {
            // Parse command-line arguments
            let schema_path = sub_matches
                .get_one::<String>("schema")
                .expect("Schema is required");

            let output_format = match sub_matches
                .get_one::<String>("format")
                .unwrap_or(&"text".to_string())
                .as_str()
            {
                "json" => ReportFormat::Json,
                _ => ReportFormat::Text,
            };

            let strict_mode = sub_matches.get_flag("strict");
            let directory_mode = sub_matches.get_flag("directory");

            // Load schema
            let schema = match Schema::from_file(schema_path) {
                Ok(schema) => schema,
                Err(err) => return RunResult::Failure(err),
            };

            // Get configuration path(s)
            let config_paths: Vec<&String> = sub_matches
                .get_many::<String>("config")
                .expect("Config is required")
                .collect();

            // Process each configuration file
            if directory_mode {
                validate_directories(&config_paths, &schema, strict_mode, &output_format)
            } else {
                validate_configs(&config_paths, &schema, strict_mode, &output_format)
            }
        }
        _ => {
            // This shouldn't happen with subcommand_required(true)
            RunResult::Failure(ConfigGuardError::Cli("No subcommand provided".to_string()))
        }
    }
}

/// Validate a list of individual configuration files
fn validate_configs(
    config_paths: &[&String],
    schema: &Schema,
    strict: bool,
    format: &ReportFormat,
) -> RunResult {
    let mut errors_found = false;

    for config_path in config_paths {
        match validate_single_config(config_path, schema, strict, format) {
            Ok(false) => {
                // Valid
                if config_paths.len() > 1 && *format == ReportFormat::Text {
                    println!("✅ {}: Valid", config_path);
                }
            }
            Ok(true) => {
                // Invalid but not a failure
                errors_found = true;
                if config_paths.len() > 1 && *format == ReportFormat::Text {
                    println!("❌ {}: Invalid", config_path);
                }
            }
            Err(err) => {
                // Error loading/validating
                if *format == ReportFormat::Text {
                    eprintln!("Error processing {}: {}", config_path, err);
                }
                errors_found = true;

                if config_paths.len() == 1 {
                    // If only one config was specified, propagate the error
                    return RunResult::Failure(err);
                }
            }
        }
    }

    if errors_found {
        RunResult::Failure(ConfigGuardError::ValidationErrors {
            count: 1, // We don't track the exact count here
            source: Box::new(ConfigGuardError::Validation {
                message: "One or more configurations failed validation".to_string(),
                path: "".to_string(),
                expected: "Valid configuration".to_string(),
                actual: "Invalid configuration".to_string(),
            }),
        })
    } else {
        RunResult::Success
    }
}

/// Validate a single configuration file and print results
fn validate_single_config(
    config_path: &str,
    schema: &Schema,
    strict: bool,
    output_format: &ReportFormat,
) -> ConfigGuardResult<bool> {
    // Load configuration
    let config = Config::from_file(config_path)?;

    // Validate against schema
    match validate(&config, schema, strict) {
        Ok(ValidationResult::Valid) => {
            // Generate report for valid result
            match format_validation_result(&ValidationResult::Valid, output_format) {
                Ok(report) => {
                    // Print report to stdout
                    println!("{}", report);
                    Ok(false) // No errors
                }
                Err(err) => Err(err),
            }
        }
        // This case should not happen with current implementation, but needed for exhaustive matching
        Ok(ValidationResult::Invalid(errors)) => {
            // Create report for invalid result
            let result = ValidationResult::Invalid(errors);
            match format_validation_result(&result, output_format) {
                Ok(report) => {
                    println!("{}", report);
                    Ok(true) // Has errors but not a failure
                }
                Err(err) => Err(err),
            }
        }
        // Always propagate AllValidationErrors directly to main for JSON formatting
        Err(err @ ConfigGuardError::AllValidationErrors { .. }) => Err(err),
        Err(err) => Err(err),
    }
}

/// Validate all compatible files in the given directories
fn validate_directories(
    dir_paths: &[&String],
    schema: &Schema,
    strict: bool,
    format: &ReportFormat,
) -> RunResult {
    let mut errors_found = false;
    let mut processed_files = 0;
    let mut valid_files = 0;
    let mut skipped_files = 0;
    let mut results = Vec::new();

    for dir_path in dir_paths {
        match process_directory(dir_path, schema, strict, format) {
            Ok((processed, valid, skipped)) => {
                processed_files += processed;
                valid_files += valid;
                skipped_files += skipped;

                results.push(serde_json::json!({
                    "directory": dir_path,
                    "processed": processed,
                    "valid": valid,
                    "invalid": processed - valid,
                    "skipped": skipped
                }));

                if processed > 0 && processed != valid {
                    errors_found = true;
                }
            }
            Err(err) => {
                if *format == ReportFormat::Text {
                    eprintln!("Error processing directory {}: {}", dir_path, err);
                }
                errors_found = true;
            }
        }
    }

    // Print summary based on output format
    if *format == ReportFormat::Text {
        println!("\nValidation Summary:");
        println!("  Processed: {} files", processed_files);
        println!("  Valid: {} files", valid_files);
        println!("  Invalid: {} files", processed_files - valid_files);
        println!(
            "  Skipped: {} files (incompatible extension)",
            skipped_files
        );
    } else if *format == ReportFormat::Json {
        // Only output JSON summary if not already output by process_directory
        if dir_paths.len() > 1 {
            let summary = serde_json::json!({
                "directories": results,
                "total": {
                    "processed": processed_files,
                    "valid": valid_files,
                    "invalid": processed_files - valid_files,
                    "skipped": skipped_files
                }
            });
            println!("{}", serde_json::to_string_pretty(&summary).unwrap());
        }
    }

    if errors_found {
        RunResult::Failure(ConfigGuardError::ValidationErrors {
            count: processed_files - valid_files,
            source: Box::new(ConfigGuardError::Validation {
                message: "One or more configurations failed validation".to_string(),
                path: "".to_string(),
                expected: "Valid configuration".to_string(),
                actual: "Invalid configuration".to_string(),
            }),
        })
    } else {
        RunResult::Success
    }
}

/// Process a directory and validate all compatible files
fn process_directory(
    dir_path: &str,
    schema: &Schema,
    strict: bool,
    output_format: &ReportFormat,
) -> ConfigGuardResult<(usize, usize, usize)> {
    let dir = Path::new(dir_path);

    if !dir.is_dir() {
        return Err(ConfigGuardError::FileNotFound {
            path: PathBuf::from(dir_path),
        });
    }

    let mut processed = 0;
    let mut valid = 0;
    let mut skipped = 0;

    if *output_format == ReportFormat::Text {
        println!("Processing directory: {}", dir_path);
    }

    let entries = fs::read_dir(dir).map_err(|e| ConfigGuardError::FileRead {
        path: dir_path.to_string(),
        error: e.to_string(),
    })?;

    for entry in entries {
        let entry = entry.map_err(|e| ConfigGuardError::IO(e.to_string()))?;
        let path = entry.path();

        if path.is_file() {
            if let Some(ext) = path.extension() {
                let ext_str = ext.to_string_lossy().to_lowercase();

                if ext_str == "yaml" || ext_str == "yml" || ext_str == "json" {
                    let path_str = path.to_string_lossy();

                    match validate_single_config(&path_str, schema, strict, &ReportFormat::Text) {
                        Ok(has_errors) => {
                            processed += 1;
                            if !has_errors {
                                valid += 1;
                                if *output_format == ReportFormat::Text {
                                    println!("✅ {}: Valid", path_str);
                                }
                            } else if *output_format == ReportFormat::Text {
                                println!("❌ {}: Invalid", path_str);
                            }
                        }
                        Err(err) => {
                            processed += 1;
                            if *output_format == ReportFormat::Text {
                                println!("❌ {}: Error - {}", path_str, err);
                            }
                        }
                    }
                } else {
                    skipped += 1;
                }
            }
        }
    }

    // Generate final report if JSON format is requested
    if matches!(output_format, ReportFormat::Json) && processed > 0 {
        let summary = serde_json::json!({
            "directory": dir_path,
            "processed": processed,
            "valid": valid,
            "invalid": processed - valid,
            "skipped": skipped
        });

        let summary_str = serde_json::to_string_pretty(&summary)
            .map_err(|e| ConfigGuardError::Serialization(e.to_string()))?;

        println!("{}", summary_str);
    }

    Ok((processed, valid, skipped))
}

/// Get the output format from command-line arguments
pub fn get_output_format(matches: &clap::ArgMatches) -> Option<ReportFormat> {
    match matches.subcommand() {
        Some(("validate", sub_matches)) => {
            let format = match sub_matches
                .get_one::<String>("format")
                .unwrap_or(&"text".to_string())
                .as_str()
            {
                "json" => ReportFormat::Json,
                _ => ReportFormat::Text,
            };
            Some(format)
        }
        _ => None,
    }
}
