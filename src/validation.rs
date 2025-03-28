use crate::config::{Config, ConfigFormat};
use crate::error::{ConfigGuardError, ConfigGuardResult};
use crate::schema::{Schema, SchemaRule, SchemaType};
use regex::Regex;
use serde_yaml::Value;

/// Represents a validation error
#[derive(Debug, Clone)]
pub struct ValidationError {
    /// Path to the error location in the configuration
    pub path: String,

    /// Message describing the validation error
    pub message: String,

    /// Expected value/type/constraint
    pub expected: String,

    /// Actual value found
    pub actual: String,

    /// Field description from schema (if available)
    pub description: Option<String>,

    /// Line number in the source file (if available)
    pub line: Option<usize>,
}

/// Result of a validation operation
#[derive(Debug, Clone)]
pub enum ValidationResult {
    /// Configuration is valid
    Valid,

    /// Configuration has validation errors
    #[allow(dead_code)]
    Invalid(Vec<ValidationError>),
}

/// Validate a configuration against a schema
pub fn validate(
    config: &Config,
    schema: &Schema,
    strict: bool,
) -> ConfigGuardResult<ValidationResult> {
    let mut errors = Vec::new();

    validate_node(&config.data, &schema.root, "", &mut errors, strict)?;

    // If we have the original content, try to find line numbers for each error
    if let Some(content) = &config.content {
        find_line_numbers(content, &mut errors, config.format);
    }

    if errors.is_empty() {
        Ok(ValidationResult::Valid)
    } else {
        // Always return all errors using the AllValidationErrors type
        Err(ConfigGuardError::AllValidationErrors { errors })
    }
}

/// Find line numbers for validation errors based on the path
fn find_line_numbers(content: &str, errors: &mut [ValidationError], format: ConfigFormat) {
    // Create a map of paths to line numbers
    let mut path_to_line = std::collections::HashMap::new();

    let lines: Vec<&str> = content.lines().collect();

    match format {
        ConfigFormat::Yaml => {
            // For YAML, we look for keys that match our paths
            for (i, line) in lines.iter().enumerate() {
                let trimmed = line.trim();
                if let Some(key) = trimmed.split(':').next() {
                    let key = key.trim();
                    if !key.is_empty() && !key.starts_with('#') {
                        // Store both the simple key and potential path components
                        path_to_line.insert(key.to_string(), i + 1);

                        // Also try to match array indices like [0]
                        if key.ends_with(']') {
                            if let Some(base_key) = key.split('[').next() {
                                path_to_line.insert(base_key.to_string(), i + 1);
                            }
                        }
                    }
                }
            }
        }
        ConfigFormat::Json => {
            // For JSON, we need a more sophisticated approach
            // This is a simplified version that looks for key patterns
            for (i, line) in lines.iter().enumerate() {
                let trimmed = line.trim();
                if trimmed.contains('"') && trimmed.contains(':') {
                    if let Some(key) = trimmed.split(':').next() {
                        let key = key.trim().trim_matches('"').trim_matches('"');
                        if !key.is_empty() {
                            path_to_line.insert(key.to_string(), i + 1);
                        }
                    }
                }
            }
        }
    }

    // Update each error with its line number if we can find it
    for error in errors {
        // Extract the last component of the path
        if let Some(last_component) = error.path.split('.').last() {
            // Remove array indices for matching
            let clean_component = if last_component.contains('[') {
                last_component.split('[').next().unwrap_or(last_component)
            } else {
                last_component
            };

            if let Some(&line) = path_to_line.get(clean_component) {
                error.line = Some(line);
            }
        }
    }
}

/// Validate a single node in the configuration
fn validate_node(
    value: &Value,
    rule: &SchemaRule,
    path: &str,
    errors: &mut Vec<ValidationError>,
    strict: bool,
) -> ConfigGuardResult<()> {
    // Adjust allow_unknown_keys based on strict mode
    // In strict mode, we don't allow unknown keys regardless of the schema setting
    let allow_unknown_keys = if strict {
        false
    } else {
        rule.allow_unknown_keys
    };

    // Type validation first
    if !validate_type(value, &rule.data_type) {
        errors.push(ValidationError {
            path: path.to_string(),
            message: "Type mismatch".to_string(),
            expected: format!("{:?}", rule.data_type),
            actual: value_type_name(value),
            description: rule.description.clone(),
            line: None,
        });
        // Don't proceed with further checks if type doesn't match
        return Ok(());
    }

    // Type-specific validation
    match rule.data_type {
        SchemaType::Object => {
            validate_object(value, rule, path, errors, allow_unknown_keys)?;
        }
        SchemaType::List => {
            validate_list(value, rule, path, errors)?;
        }
        SchemaType::String => {
            validate_string(value, rule, path, errors)?;
        }
        SchemaType::Integer | SchemaType::Float => {
            validate_number(value, rule, path, errors)?;
        }
        SchemaType::Boolean => {
            // Already validated by type check
        }
        SchemaType::Null => {
            // Already validated by type check
        }
        SchemaType::Any => {
            // Any type is always valid
        }
    }

    Ok(())
}

/// Validate an object node against a schema rule
fn validate_object(
    value: &Value,
    rule: &SchemaRule,
    path: &str,
    errors: &mut Vec<ValidationError>,
    allow_unknown_keys: bool,
) -> ConfigGuardResult<()> {
    if let Value::Mapping(map) = value {
        // Check if all required keys are present
        if let Some(keys) = &rule.keys {
            for (key_name, key_rule) in keys {
                if key_rule.required && !map.contains_key(Value::String(key_name.clone())) {
                    let field_desc = key_rule.description.clone();
                    errors.push(ValidationError {
                        path: if path.is_empty() {
                            format!(".{}", key_name)
                        } else {
                            format!("{}.{}", path, key_name)
                        },
                        message: "Required key missing".to_string(),
                        expected: "Key to be present".to_string(),
                        actual: "Key is absent".to_string(),
                        description: field_desc,
                        line: None,
                    });
                } else if key_rule.required && key_rule.data_type == SchemaType::Object {
                    // Check if the required object is empty when it shouldn't be
                    if let Some(Value::Mapping(inner_map)) =
                        map.get(&Value::String(key_name.clone()))
                    {
                        if inner_map.is_empty()
                            && key_rule.keys.is_some()
                            && !key_rule.keys.as_ref().unwrap().is_empty()
                        {
                            let field_desc = key_rule.description.clone();
                            errors.push(ValidationError {
                                path: if path.is_empty() {
                                    format!(".{}", key_name)
                                } else {
                                    format!("{}.{}", path, key_name)
                                },
                                message: "Required object is empty".to_string(),
                                expected: "Object with required fields".to_string(),
                                actual: "Empty object".to_string(),
                                description: field_desc,
                                line: None,
                            });
                        }
                    }
                }
            }

            // Check each key in the configuration
            for (key, val) in map {
                if let Value::String(key_name) = key {
                    // Check if the key is defined in the schema
                    if let Some(key_rule) = keys.get(key_name) {
                        // Validate the value against the key's rule
                        let new_path = if path.is_empty() {
                            format!(".{}", key_name)
                        } else {
                            format!("{}.{}", path, key_name)
                        };

                        // Pass down the strict mode setting to nested validations
                        // If we're in strict mode (allow_unknown_keys is false), pass that down
                        // Otherwise use the key rule's setting
                        let nested_allow_unknown = if !allow_unknown_keys {
                            false // Strict mode
                        } else {
                            key_rule.allow_unknown_keys
                        };
                        validate_node(val, key_rule, &new_path, errors, nested_allow_unknown)?;
                    } else if !allow_unknown_keys {
                        // Report unknown key error if in strict mode
                        errors.push(ValidationError {
                            path: if path.is_empty() {
                                format!(".{}", key_name)
                            } else {
                                format!("{}.{}", path, key_name)
                            },
                            message: "Unknown key".to_string(),
                            expected: "Key defined in schema".to_string(),
                            actual: "Undefined key".to_string(),
                            description: None,
                            line: None,
                        });
                    }
                }
            }
        }
    }

    Ok(())
}

/// Validate a list node against a schema rule
fn validate_list(
    value: &Value,
    rule: &SchemaRule,
    path: &str,
    errors: &mut Vec<ValidationError>,
) -> ConfigGuardResult<()> {
    if let Value::Sequence(items) = value {
        // Check list length constraints
        if let Some(min_length) = rule.min_length {
            if items.len() < min_length {
                errors.push(ValidationError {
                    path: path.to_string(),
                    message: "List too short".to_string(),
                    expected: format!("At least {} items", min_length),
                    actual: format!("{} items", items.len()),
                    description: rule.description.clone(),
                    line: None,
                });

                // If the list is empty and items are required, don't try to validate items
                if items.is_empty() && min_length > 0 {
                    return Ok(());
                }
            }
        }

        if let Some(max_length) = rule.max_length {
            if items.len() > max_length {
                errors.push(ValidationError {
                    path: path.to_string(),
                    message: "List too long".to_string(),
                    expected: format!("At most {} items", max_length),
                    actual: format!("{} items", items.len()),
                    description: rule.description.clone(),
                    line: None,
                });
            }
        }

        // Validate each item if a rule is defined
        if let Some(item_rule) = &rule.items {
            for (i, item) in items.iter().enumerate() {
                let item_path = format!("{}[{}]", path, i);
                validate_node(item, item_rule, &item_path, errors, false)?;
            }
        }
    }

    Ok(())
}

/// Validate a string node against a schema rule
fn validate_string(
    value: &Value,
    rule: &SchemaRule,
    path: &str,
    errors: &mut Vec<ValidationError>,
) -> ConfigGuardResult<()> {
    if let Value::String(s) = value {
        // Check string length constraints
        if let Some(min_length) = rule.min_length {
            if s.len() < min_length {
                errors.push(ValidationError {
                    path: path.to_string(),
                    message: "String too short".to_string(),
                    expected: format!("At least {} characters", min_length),
                    actual: format!("{} characters", s.len()),
                    description: rule.description.clone(),
                    line: None,
                });
            }
        }

        if let Some(max_length) = rule.max_length {
            if s.len() > max_length {
                errors.push(ValidationError {
                    path: path.to_string(),
                    message: "String too long".to_string(),
                    expected: format!("At most {} characters", max_length),
                    actual: format!("{} characters", s.len()),
                    description: rule.description.clone(),
                    line: None,
                });
            }
        }

        // Check pattern constraint
        if let Some(pattern) = &rule.pattern {
            let regex = Regex::new(pattern).map_err(|e| {
                ConfigGuardError::Pattern(format!("Invalid pattern in schema: {}", e))
            })?;

            if !regex.is_match(s) {
                errors.push(ValidationError {
                    path: path.to_string(),
                    message: "String doesn't match pattern".to_string(),
                    expected: format!("Pattern: {}", pattern),
                    actual: s.clone(),
                    description: rule.description.clone(),
                    line: None,
                });
            }
        }

        // Check enum constraint
        if let Some(enum_values) = &rule.enum_values {
            let mut found = false;

            for enum_val in enum_values {
                if let Value::String(enum_str) = enum_val {
                    if s == enum_str {
                        found = true;
                        break;
                    }
                }
            }

            if !found {
                let allowed_values = enum_values
                    .iter()
                    .map(|v| format!("{:?}", v))
                    .collect::<Vec<_>>()
                    .join(", ");

                errors.push(ValidationError {
                    path: path.to_string(),
                    message: "Value not in allowed set".to_string(),
                    expected: format!("One of: {}", allowed_values),
                    actual: s.clone(),
                    description: rule.description.clone(),
                    line: None,
                });
            }
        }
    }

    Ok(())
}

/// Validate a numeric node against a schema rule
fn validate_number(
    value: &Value,
    rule: &SchemaRule,
    path: &str,
    errors: &mut Vec<ValidationError>,
) -> ConfigGuardResult<()> {
    // Helper function to get a numeric value as f64
    fn as_f64(value: &Value) -> Option<f64> {
        match value {
            Value::Number(n) => n.as_f64(),
            _ => None,
        }
    }

    if let Some(num) = as_f64(value) {
        // Check for NaN or infinite values
        if num.is_nan() {
            errors.push(ValidationError {
                path: path.to_string(),
                message: "Invalid numeric value".to_string(),
                expected: "A valid number".to_string(),
                actual: "NaN (Not a Number)".to_string(),
                description: rule.description.clone(),
                line: None,
            });
            return Ok(());
        }

        if num.is_infinite() {
            errors.push(ValidationError {
                path: path.to_string(),
                message: "Invalid numeric value".to_string(),
                expected: "A finite number".to_string(),
                actual: if num.is_sign_positive() {
                    "Positive infinity"
                } else {
                    "Negative infinity"
                }
                .to_string(),
                description: rule.description.clone(),
                line: None,
            });
            return Ok(());
        }

        // Check min constraint
        if let Some(min) = &rule.min {
            if let Some(min_val) = min.as_f64() {
                // Use <= for inclusive minimum check
                if num < min_val {
                    errors.push(ValidationError {
                        path: path.to_string(),
                        message: "Value too small".to_string(),
                        expected: format!("At least {}", min_val),
                        actual: format!("{}", num),
                        description: rule.description.clone(),
                        line: None,
                    });
                }
            }
        }

        // Check max constraint
        if let Some(max) = &rule.max {
            if let Some(max_val) = max.as_f64() {
                if num > max_val {
                    errors.push(ValidationError {
                        path: path.to_string(),
                        message: "Value too large".to_string(),
                        expected: format!("At most {}", max_val),
                        actual: format!("{}", num),
                        description: rule.description.clone(),
                        line: None,
                    });
                }
            }
        }

        // Check enum constraint
        if let Some(enum_values) = &rule.enum_values {
            let mut found = false;

            for enum_val in enum_values {
                if let Some(enum_num) = as_f64(enum_val) {
                    // More robust floating-point comparison
                    if (num - enum_num).abs() <= f64::EPSILON {
                        found = true;
                        break;
                    }
                } else {
                    // Add an error for non-numeric enum values when validating numbers
                    errors.push(ValidationError {
                        path: path.to_string(),
                        message: "Invalid enum value type".to_string(),
                        expected: "Numeric value for numeric field".to_string(),
                        actual: format!("Non-numeric value: {:?}", enum_val),
                        description: rule.description.clone(),
                        line: None,
                    });
                    // Don't continue checking other enum values if we found an invalid type
                    return Ok(());
                }
            }

            if !found {
                let allowed_values = enum_values
                    .iter()
                    .map(|v| format!("{:?}", v))
                    .collect::<Vec<_>>()
                    .join(", ");

                errors.push(ValidationError {
                    path: path.to_string(),
                    message: "Value not in allowed set".to_string(),
                    expected: format!("One of: {}", allowed_values),
                    actual: format!("{}", num),
                    description: rule.description.clone(),
                    line: None,
                });
            }
        }
    }

    Ok(())
}

/// Check if a value matches the expected type
fn validate_type(value: &Value, expected_type: &SchemaType) -> bool {
    match expected_type {
        SchemaType::String => matches!(value, Value::String(_)),
        SchemaType::Integer => {
            if let Value::Number(n) = value {
                n.is_i64() || n.is_u64()
            } else {
                false
            }
        }
        SchemaType::Float => {
            if let Value::Number(n) = value {
                n.is_f64()
            } else {
                false
            }
        }
        SchemaType::Boolean => matches!(value, Value::Bool(_)),
        SchemaType::Object => matches!(value, Value::Mapping(_)),
        SchemaType::List => matches!(value, Value::Sequence(_)),
        SchemaType::Null => matches!(value, Value::Null),
        SchemaType::Any => true,
    }
}

/// Get a human-readable name for a value's type
fn value_type_name(value: &Value) -> String {
    match value {
        Value::String(_) => "string".to_string(),
        Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                "integer".to_string()
            } else {
                "float".to_string()
            }
        }
        Value::Bool(_) => "boolean".to_string(),
        Value::Mapping(_) => "object".to_string(),
        Value::Sequence(_) => "list".to_string(),
        Value::Null => "null".to_string(),
        _ => "unknown".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, ConfigFormat};
    use crate::schema::{Schema, SchemaRule, SchemaType};
    use std::collections::HashMap;

    // Helper to create a Config from YAML string
    fn config_from_yaml(yaml: &str) -> Config {
        let data = serde_yaml::from_str(yaml).unwrap();
        Config {
            data,
            format: ConfigFormat::Yaml,
            path: None,
            content: Some(yaml.to_string()),
        }
    }

    // Helper to create a Schema from YAML string
    fn schema_from_yaml(yaml: &str) -> Schema {
        let rule: SchemaRule = serde_yaml::from_str(yaml).unwrap();
        Schema { root: rule }
    }

    #[test]
    fn test_validate_valid_simple_config() {
        let schema_str = r#"
            type: object
            keys:
                name:
                    type: string
                    required: true
                age:
                    type: integer
                    min: 0
                    max: 120
        "#;

        let config_str = r#"
            name: John Doe
            age: 30
        "#;

        let schema = schema_from_yaml(schema_str);
        let config = config_from_yaml(config_str);

        let result = validate(&config, &schema, false).unwrap();
        assert!(matches!(result, ValidationResult::Valid));
    }

    #[test]
    fn test_validate_missing_required_field() {
        let schema_str = r#"
            type: object
            keys:
                name:
                    type: string
                    required: true
                age:
                    type: integer
                    required: true
        "#;

        let config_str = r#"
            name: John Doe
            # age is missing
        "#;

        let schema = schema_from_yaml(schema_str);
        let config = config_from_yaml(config_str);

        let result = validate(&config, &schema, false);

        assert!(result.is_err());
        match result {
            Ok(_) => panic!("Validation should have failed due to missing required field"),
            Err(err) => match err {
                ConfigGuardError::AllValidationErrors { errors } => {
                    assert_eq!(errors.len(), 1);
                    assert_eq!(errors[0].path, ".age");
                    assert_eq!(errors[0].message, "Required key missing");
                }
                _ => panic!("Expected AllValidationErrors, got {:?}", err),
            },
        }
    }

    #[test]
    fn test_validate_type_mismatch() {
        let schema_str = r#"
            type: object
            keys:
                name:
                    type: string
                age:
                    type: integer
        "#;

        let config_str = r#"
            name: John Doe
            age: "thirty" # Should be an integer
        "#;

        let schema = schema_from_yaml(schema_str);
        let config = config_from_yaml(config_str);

        let result = validate(&config, &schema, false);

        assert!(result.is_err());
        match result {
            Ok(_) => panic!("Validation should have failed due to type mismatch"),
            Err(err) => match err {
                ConfigGuardError::AllValidationErrors { errors } => {
                    assert_eq!(errors.len(), 1);
                    assert_eq!(errors[0].path, ".age");
                    assert_eq!(errors[0].message, "Type mismatch");
                }
                _ => panic!("Expected AllValidationErrors, got {:?}", err),
            },
        }
    }

    #[test]
    fn test_validate_range_constraints() {
        let schema_str = r#"
            type: object
            keys:
                count:
                    type: integer
                    min: 1
                    max: 10
        "#;

        // Test value too small
        let config_str = "count: 0"; // Below minimum
        let schema = schema_from_yaml(schema_str);
        let config = config_from_yaml(config_str);

        let result = validate(&config, &schema, false);
        assert!(result.is_err());
        match result {
            Ok(_) => panic!("Validation should have failed due to value below minimum"),
            Err(err) => match err {
                ConfigGuardError::AllValidationErrors { errors } => {
                    assert_eq!(errors.len(), 1);
                    assert_eq!(errors[0].message, "Value too small");
                }
                _ => panic!("Expected AllValidationErrors, got {:?}", err),
            },
        }

        // Test value too large
        let config_str = "count: 11"; // Above maximum
        let config = config_from_yaml(config_str);

        let result = validate(&config, &schema, false);
        assert!(result.is_err());
        match result {
            Ok(_) => panic!("Validation should have failed due to value above maximum"),
            Err(err) => match err {
                ConfigGuardError::AllValidationErrors { errors } => {
                    assert_eq!(errors.len(), 1);
                    assert_eq!(errors[0].message, "Value too large");
                }
                _ => panic!("Expected AllValidationErrors, got {:?}", err),
            },
        }
    }

    #[test]
    fn test_validate_string_pattern() {
        let schema_str = r#"
            type: object
            keys:
                code:
                    type: string
                    pattern: "^[A-Z]{3}-\\d{4}$"
        "#;

        // Valid pattern
        let config_str = "code: ABC-1234";
        let schema = schema_from_yaml(schema_str);
        let config = config_from_yaml(config_str);

        let result = validate(&config, &schema, false);
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), ValidationResult::Valid));

        // Invalid pattern
        let config_str = "code: abc-123"; // Doesn't match pattern
        let config = config_from_yaml(config_str);

        let result = validate(&config, &schema, false);
        assert!(result.is_err());
        match result {
            Ok(_) => panic!("Validation should have failed due to pattern mismatch"),
            Err(err) => match err {
                ConfigGuardError::AllValidationErrors { errors } => {
                    assert_eq!(errors.len(), 1);
                    assert_eq!(errors[0].message, "String doesn't match pattern");
                }
                _ => panic!("Expected AllValidationErrors, got {:?}", err),
            },
        }
    }

    #[test]
    fn test_validate_enum_values() {
        let schema_str = r#"
            type: object
            keys:
                color:
                    type: string
                    enum: [red, green, blue]
        "#;

        // Valid enum value
        let config_str = "color: green";
        let schema = schema_from_yaml(schema_str);
        let config = config_from_yaml(config_str);

        let result = validate(&config, &schema, false);
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), ValidationResult::Valid));

        // Invalid enum value
        let config_str = "color: yellow"; // Not in enum
        let config = config_from_yaml(config_str);

        let result = validate(&config, &schema, false);
        assert!(result.is_err());
        match result {
            Ok(_) => panic!("Validation should have failed due to invalid enum value"),
            Err(err) => match err {
                ConfigGuardError::AllValidationErrors { errors } => {
                    assert_eq!(errors.len(), 1);
                    assert_eq!(errors[0].message, "Value not in allowed set");
                }
                _ => panic!("Expected AllValidationErrors, got {:?}", err),
            },
        }
    }

    #[test]
    fn test_validate_strict_mode() {
        let schema_str = r#"
            type: object
            keys:
                name:
                    type: string
                    required: true
        "#;

        let config_str = r#"
            name: Test
            extra_field: This shouldn't be here
        "#;

        let schema = schema_from_yaml(schema_str);
        let config = config_from_yaml(config_str);

        // With strict mode off (default), unknown keys are allowed
        let result = validate(&config, &schema, false);
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), ValidationResult::Valid));

        // With strict mode on, unknown keys should cause a validation error
        let result = validate(&config, &schema, true);
        assert!(result.is_err());
        match result {
            Ok(_) => panic!("Validation should have failed in strict mode due to unknown keys"),
            Err(err) => match err {
                ConfigGuardError::AllValidationErrors { errors } => {
                    assert_eq!(errors.len(), 1);
                    assert_eq!(errors[0].path, ".extra_field");
                    assert_eq!(errors[0].message, "Unknown key");
                }
                _ => panic!("Expected AllValidationErrors, got {:?}", err),
            },
        }
    }

    #[test]
    fn test_validate_nested_structures() {
        // Create a schema with nested structure requirements
        let mut schema_keys = HashMap::new();

        // Add the metadata object with required name field
        let mut metadata_keys = HashMap::new();
        metadata_keys.insert(
            "name".to_string(),
            SchemaRule {
                data_type: SchemaType::String,
                description: Some("The name of the resource".to_string()),
                required: true,
                allow_unknown_keys: true,
                pattern: None,
                enum_values: None,
                min: None,
                max: None,
                min_length: None,
                max_length: None,
                items: None,
                keys: None,
            },
        );

        schema_keys.insert(
            "metadata".to_string(),
            SchemaRule {
                data_type: SchemaType::Object,
                description: Some("Metadata for the resource".to_string()),
                required: true,
                allow_unknown_keys: true,
                pattern: None,
                enum_values: None,
                min: None,
                max: None,
                min_length: None,
                max_length: None,
                items: None,
                keys: Some(metadata_keys),
            },
        );

        // Add a containers list with minimum length requirement
        schema_keys.insert(
            "containers".to_string(),
            SchemaRule {
                data_type: SchemaType::List,
                description: Some("List of containers".to_string()),
                required: true,
                allow_unknown_keys: true,
                pattern: None,
                enum_values: None,
                min: None,
                max: None,
                min_length: Some(1),
                max_length: None,
                items: Some(Box::new(SchemaRule {
                    data_type: SchemaType::Object,
                    description: None,
                    required: false,
                    allow_unknown_keys: true,
                    pattern: None,
                    enum_values: None,
                    min: None,
                    max: None,
                    min_length: None,
                    max_length: None,
                    items: None,
                    keys: None,
                })),
                keys: None,
            },
        );

        let schema = Schema {
            root: SchemaRule {
                data_type: SchemaType::Object,
                description: None,
                required: false,
                allow_unknown_keys: true,
                pattern: None,
                enum_values: None,
                min: None,
                max: None,
                min_length: None,
                max_length: None,
                items: None,
                keys: Some(schema_keys),
            },
        };

        // Valid nested config
        let valid_nested_config = r#"
            metadata:
              name: test-app
            containers:
              - name: web
                image: nginx
        "#;

        let config = Config {
            data: serde_yaml::from_str(valid_nested_config).unwrap(),
            format: ConfigFormat::Yaml,
            path: None,
            content: Some(valid_nested_config.to_string()),
        };

        let result = validate(&config, &schema, false);
        assert!(result.is_ok());
        match result.unwrap() {
            ValidationResult::Valid => {}
            ValidationResult::Invalid(_) => panic!("Expected valid result"),
        }

        // Invalid nested config - missing required field
        let invalid_nested_config = r#"
            metadata: {}
            containers:
              - name: web
                image: nginx
        "#;

        let config = Config {
            data: serde_yaml::from_str(invalid_nested_config).unwrap(),
            format: ConfigFormat::Yaml,
            path: None,
            content: Some(invalid_nested_config.to_string()),
        };

        let result = validate(&config, &schema, false);
        assert!(result.is_err());
        match result {
            Ok(_) => panic!("Expected validation error"),
            Err(err) => match err {
                ConfigGuardError::AllValidationErrors { errors } => {
                    // We're getting two errors: one for the missing key and one for the empty object
                    // Find the error for the missing required field
                    let missing_key_error =
                        errors.iter().find(|e| e.message == "Required key missing");
                    assert!(missing_key_error.is_some());
                    let error = missing_key_error.unwrap();
                    assert_eq!(error.path, ".metadata.name");
                    assert_eq!(error.message, "Required key missing");
                }
                _ => panic!("Expected AllValidationErrors, got {:?}", err),
            },
        }

        // Invalid nested config - empty list
        let empty_list_config = r#"
            metadata:
              name: test-app
            containers: []
        "#;

        let config = Config {
            data: serde_yaml::from_str(empty_list_config).unwrap(),
            format: ConfigFormat::Yaml,
            path: None,
            content: Some(empty_list_config.to_string()),
        };

        let result = validate(&config, &schema, false);
        assert!(result.is_err());
        match result {
            Ok(_) => panic!("Expected validation error"),
            Err(err) => match err {
                ConfigGuardError::AllValidationErrors { errors } => {
                    assert_eq!(errors.len(), 1);
                    assert_eq!(errors[0].path, ".containers");
                    assert_eq!(errors[0].message, "List too short");
                }
                _ => panic!("Expected AllValidationErrors, got {:?}", err),
            },
        }
    }
}
