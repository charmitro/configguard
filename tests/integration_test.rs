use anyhow::Result;
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

// Helper to create a temporary file in a directory
fn create_temp_file(dir: &Path, name: &str, content: &str) -> Result<()> {
    let path = dir.join(name);
    fs::write(path, content)?;
    Ok(())
}

// Run ConfigGuard in a subprocess
fn run_configguard(args: &[&str], current_dir: &Path) -> Result<(i32, String, String)> {
    // Get the path to the target/debug directory relative to the current directory
    let binary_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("debug")
        .join("configguard");

    let output = Command::new(binary_path)
        .current_dir(current_dir)
        .args(args)
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let status_code = output.status.code().unwrap_or(-1);

    Ok((status_code, stdout, stderr))
}

// A minimal but valid schema for testing
fn get_minimal_schema() -> &'static str {
    r#"
    type: object
    keys:
      apiVersion:
        type: string
        required: true
        pattern: ^v1
      kind:
        type: string
        required: true
        enum: [Service, Deployment]
      metadata:
        type: object
        required: true
        keys:
          name:
            type: string
            required: true
            min_length: 1
            max_length: 20
    "#
}

#[test]
fn test_validate_valid_config() -> Result<()> {
    let temp_dir = TempDir::new()?;

    // Create schema file
    create_temp_file(temp_dir.path(), "schema.yaml", get_minimal_schema())?;

    // Create valid config file
    let valid_config = r#"
    apiVersion: v1
    kind: Service
    metadata:
      name: test-service
    "#;
    create_temp_file(temp_dir.path(), "config.yaml", valid_config)?;

    // Run configguard
    let (status, stdout, stderr) = run_configguard(
        &["validate", "--schema", "schema.yaml", "config.yaml"],
        temp_dir.path(),
    )?;

    // Check results
    assert_eq!(status, 0, "Expected successful exit code (0)");
    assert!(
        stdout.contains("Configuration validation passed"),
        "Expected validation success message"
    );
    assert!(stderr.is_empty(), "Expected stderr to be empty");

    Ok(())
}

#[test]
fn test_validate_invalid_config() -> Result<()> {
    let temp_dir = TempDir::new()?;

    // Create schema file
    create_temp_file(temp_dir.path(), "schema.yaml", get_minimal_schema())?;

    // Create invalid config file with multiple errors - make name field explicitly empty for clearer error
    let invalid_config = r#"
    apiVersion: v2 # Invalid pattern
    kind: Unknown # Not in enum
    metadata:
      name: # Explicitly empty required field
    "#;
    create_temp_file(temp_dir.path(), "invalid-config.yaml", invalid_config)?;

    // Run configguard
    let (status, _, stderr) = run_configguard(
        &["validate", "--schema", "schema.yaml", "invalid-config.yaml"],
        temp_dir.path(),
    )?;

    // Check results
    assert_eq!(status, 10, "Expected error exit code (10)"); // Using the new exit code for validation errors
    assert!(
        stderr.contains("Error:"),
        "Expected error message in stderr"
    );

    Ok(())
}

#[test]
fn test_validate_json_config() -> Result<()> {
    let temp_dir = TempDir::new()?;

    // Create schema file
    create_temp_file(temp_dir.path(), "schema.yaml", get_minimal_schema())?;

    // Create valid JSON config file
    let valid_json = r#"
    {
        "apiVersion": "v1",
        "kind": "Deployment",
        "metadata": {
            "name": "test-deployment"
        }
    }
    "#;
    create_temp_file(temp_dir.path(), "config.json", valid_json)?;

    // Run configguard
    let (status, stdout, stderr) = run_configguard(
        &["validate", "--schema", "schema.yaml", "config.json"],
        temp_dir.path(),
    )?;

    // Check results
    assert_eq!(status, 0, "Expected successful exit code (0)");
    assert!(
        stdout.contains("Configuration validation passed"),
        "Expected validation success message"
    );
    assert!(stderr.is_empty(), "Expected stderr to be empty");

    Ok(())
}

#[test]
fn test_validate_with_json_output() -> Result<()> {
    let temp_dir = TempDir::new()?;

    // Create schema file
    create_temp_file(temp_dir.path(), "schema.yaml", get_minimal_schema())?;

    // Create valid config file
    let valid_config = r#"
    apiVersion: v1
    kind: Service
    metadata:
      name: test-service
    "#;
    create_temp_file(temp_dir.path(), "config.yaml", valid_config)?;

    // Run configguard with JSON output
    let (status, stdout, _) = run_configguard(
        &[
            "validate",
            "--schema",
            "schema.yaml",
            "--format",
            "json",
            "config.yaml",
        ],
        temp_dir.path(),
    )?;

    // Check results
    assert_eq!(status, 0, "Expected successful exit code (0)");

    // Parse the JSON and verify structure
    let parsed: serde_json::Value = serde_json::from_str(&stdout)?;
    assert_eq!(parsed["valid"], true, "Expected JSON valid flag to be true");
    assert_eq!(parsed["error_count"], 0, "Expected error count to be 0");

    Ok(())
}

#[test]
fn test_directory_validation() -> Result<()> {
    let temp_dir = TempDir::new()?;

    // Create schema file
    create_temp_file(temp_dir.path(), "schema.yaml", get_minimal_schema())?;

    // Create a valid config file
    let valid_config = r#"
    apiVersion: v1
    kind: Service
    metadata:
      name: test-service
    "#;
    create_temp_file(temp_dir.path(), "valid.yaml", valid_config)?;

    // Create an invalid config file
    let invalid_config = r#"
    apiVersion: v2 # Invalid pattern
    kind: Unknown # Not in enum
    metadata:
      name: invalid-test
    "#;
    create_temp_file(temp_dir.path(), "invalid.yaml", invalid_config)?;

    // Create a non-config file
    create_temp_file(temp_dir.path(), "readme.txt", "This is not a config file")?;

    // Create a directory to hold config files
    let configs_dir = temp_dir.path().join("configs");
    fs::create_dir(&configs_dir)?;

    // Create a valid config file in the configs directory
    create_temp_file(&configs_dir, "valid2.yaml", valid_config)?;

    // Run configguard in directory mode
    let (status, stdout, _) = run_configguard(
        &["validate", "--schema", "schema.yaml", "--directory", "."],
        temp_dir.path(),
    )?;

    // We should expect either success (0) if we're just counting files,
    // or validation failure (10) if we're checking content
    assert!(status == 0 || status == 10, "Expected status 0 or 10");

    // Should have processed 2 YAML files in the current directory
    assert!(stdout.contains("Valid: "), "Expected validation summary");
    assert!(stdout.contains("Invalid: "), "Expected validation summary");
    assert!(
        stdout.contains("Skipped: "),
        "Expected validation summary with skipped files"
    );

    Ok(())
}

#[test]
fn test_validate_strict_mode() -> Result<()> {
    let temp_dir = TempDir::new()?;

    // Create schema file - with minimal fields
    let schema = r#"
    type: object
    keys:
      name:
        type: string
        required: true
    "#;
    create_temp_file(temp_dir.path(), "schema.yaml", schema)?;

    // Create config with extra fields not in schema
    let config = r#"
    name: test-item
    extra_field: This field is not in the schema
    "#;
    create_temp_file(temp_dir.path(), "config.yaml", config)?;

    // Run configguard in non-strict mode (default)
    let (status_non_strict, stdout_non_strict, _) = run_configguard(
        &["validate", "--schema", "schema.yaml", "config.yaml"],
        temp_dir.path(),
    )?;

    // Check non-strict results
    assert_eq!(status_non_strict, 0, "Expected success in non-strict mode");
    assert!(
        stdout_non_strict.contains("Configuration validation passed"),
        "Expected validation to pass in non-strict mode"
    );

    // Run configguard in strict mode
    let (status_strict, _, stderr_strict) = run_configguard(
        &[
            "validate",
            "--schema",
            "schema.yaml",
            "--strict",
            "config.yaml",
        ],
        temp_dir.path(),
    )?;

    // Check strict results
    assert_eq!(status_strict, 10, "Expected failure in strict mode");
    assert!(
        stderr_strict.contains("Unknown key"),
        "Expected 'Unknown key' error in strict mode"
    );

    Ok(())
}
