use crate::config::Config;
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

    if errors.is_empty() {
        Ok(ValidationResult::Valid)
    } else {
        // Always return all errors using the AllValidationErrors type
        Err(ConfigGuardError::AllValidationErrors { errors })
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
                    });
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

                        validate_node(val, key_rule, &new_path, errors, allow_unknown_keys)?;
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
                });
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
        // Check min constraint
        if let Some(min) = &rule.min {
            if let Some(min_val) = min.as_f64() {
                if num < min_val {
                    errors.push(ValidationError {
                        path: path.to_string(),
                        message: "Value too small".to_string(),
                        expected: format!("At least {}", min_val),
                        actual: format!("{}", num),
                        description: rule.description.clone(),
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
                    });
                }
            }
        }

        // Check enum constraint
        if let Some(enum_values) = &rule.enum_values {
            let mut found = false;

            for enum_val in enum_values {
                if let Some(enum_num) = as_f64(enum_val) {
                    if (num - enum_num).abs() < f64::EPSILON {
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
                    actual: format!("{}", num),
                    description: rule.description.clone(),
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
        };

        let result = validate(&config, &schema, false);
        assert!(result.is_err());
        match result {
            Ok(_) => panic!("Expected validation error"),
            Err(err) => match err {
                ConfigGuardError::AllValidationErrors { errors } => {
                    assert_eq!(errors.len(), 1);
                    assert_eq!(errors[0].path, ".metadata.name");
                    assert_eq!(errors[0].message, "Required key missing");
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
