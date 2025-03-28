use crate::error::{ConfigGuardError, ConfigGuardResult};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Represents a schema for configuration validation
#[derive(Debug, Clone)]
pub struct Schema {
    pub root: SchemaRule,
}

/// The type of a schema value
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SchemaType {
    String,
    Integer,
    Float,
    Boolean,
    Object,
    List,
    Any,
    Null,
}

/// A rule in the schema definition
#[derive(Debug, Clone, Deserialize)]
pub struct SchemaRule {
    /// The expected data type
    #[serde(rename = "type")]
    pub data_type: SchemaType,

    /// Optional description of the field/node
    pub description: Option<String>,

    /// Whether the key must exist (for object fields)
    #[serde(default)]
    pub required: bool,

    /// Object-specific: Rules for child keys
    pub keys: Option<HashMap<String, SchemaRule>>,

    /// Object-specific: Whether to allow keys not defined in the schema
    #[serde(default = "default_allow_unknown_keys")]
    pub allow_unknown_keys: bool,

    /// List-specific: Validation rules for list items
    pub items: Option<Box<SchemaRule>>,

    /// List/String-specific: Minimum length
    pub min_length: Option<usize>,

    /// List/String-specific: Maximum length
    pub max_length: Option<usize>,

    /// String-specific: Regex pattern
    pub pattern: Option<String>,

    /// String/Number-specific: Enum of allowed values
    #[serde(rename = "enum")]
    pub enum_values: Option<Vec<Value>>,

    /// Number-specific: Minimum value (inclusive)
    pub min: Option<Value>,

    /// Number-specific: Maximum value (inclusive)
    pub max: Option<Value>,
}

fn default_allow_unknown_keys() -> bool {
    true
}

impl Schema {
    /// Load a schema from a file
    pub fn from_file<P: AsRef<Path>>(path: P) -> ConfigGuardResult<Self> {
        let path = path.as_ref();

        let file_content = fs::read_to_string(path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => ConfigGuardError::FileNotFound {
                path: path.to_path_buf(),
            },
            _ => ConfigGuardError::IO(e.to_string()),
        })?;

        let root: SchemaRule = match serde_yaml::from_str(&file_content) {
            Ok(rule) => rule,
            Err(e) => {
                // Provide more detailed error information for schema parsing failures
                let error_msg = if e.to_string().contains("invalid type") {
                    format!(
                        "Failed to parse schema YAML from {}: {}. Check that all types are valid (string, integer, float, boolean, object, list, any, null).",
                        path.display(),
                        e
                    )
                } else {
                    format!("Failed to parse schema YAML from {}: {}", path.display(), e)
                };
                return Err(ConfigGuardError::Schema(error_msg));
            }
        };

        // Validate the schema itself
        Self::validate_schema_rule(&root).map_err(|e| ConfigGuardError::Schema(e.to_string()))?;

        Ok(Schema { root })
    }

    /// Validate the schema itself for correctness
    fn validate_schema_rule(rule: &SchemaRule) -> ConfigGuardResult<()> {
        let context = rule.description.as_ref().map_or_else(
            || format!("for field of type {:?}", rule.data_type),
            |desc| format!("for field '{}'", desc),
        );

        // Type-specific validation
        match rule.data_type {
            SchemaType::Object => {
                // Object should have keys defined
                if let Some(keys) = &rule.keys {
                    // Recursively validate each key's rule
                    for (key_name, key_rule) in keys {
                        Self::validate_schema_rule(key_rule).map_err(|e| {
                            ConfigGuardError::Schema(format!(
                                "Invalid schema rule for key '{}': {}",
                                key_name, e
                            ))
                        })?;
                    }
                }

                // Validate object-specific properties
                if rule.items.is_some() {
                    return Err(ConfigGuardError::Schema(format!(
                        "'items' is only valid for type 'list', not 'object' {}",
                        context
                    )));
                }
                if rule.min_length.is_some() || rule.max_length.is_some() {
                    return Err(ConfigGuardError::Schema(format!(
                        "'min_length' and 'max_length' are not valid for type 'object' {}",
                        context
                    )));
                }
                if rule.pattern.is_some() {
                    return Err(ConfigGuardError::Schema(format!(
                        "'pattern' is only valid for type 'string', not 'object' {}",
                        context
                    )));
                }
                if rule.min.is_some() || rule.max.is_some() {
                    return Err(ConfigGuardError::Schema(format!(
                        "'min' and 'max' are only valid for numeric types, not 'object' {}",
                        context
                    )));
                }
            }
            SchemaType::List => {
                // Validate list-specific properties
                if rule.keys.is_some() {
                    return Err(ConfigGuardError::Schema(format!(
                        "'keys' is only valid for type 'object', not 'list' {}",
                        context
                    )));
                }
                if rule.pattern.is_some() {
                    return Err(ConfigGuardError::Schema(format!(
                        "'pattern' is only valid for type 'string', not 'list' {}",
                        context
                    )));
                }
                if rule.min.is_some() || rule.max.is_some() {
                    return Err(ConfigGuardError::Schema(format!(
                        "'min' and 'max' are only valid for numeric types, not 'list' {}",
                        context
                    )));
                }

                // If 'items' is defined, recursively validate it
                if let Some(items_rule) = &rule.items {
                    Self::validate_schema_rule(items_rule).map_err(|e| {
                        ConfigGuardError::Schema(format!(
                            "Invalid schema rule for list items {}: {}",
                            context, e
                        ))
                    })?;
                }

                // Validate min_length/max_length relationship if both are specified
                if let (Some(min), Some(max)) = (rule.min_length, rule.max_length) {
                    if min > max {
                        return Err(ConfigGuardError::Schema(format!(
                            "'min_length' cannot be greater than 'max_length' {}",
                            context
                        )));
                    }
                }
            }
            SchemaType::String => {
                // Validate string-specific properties
                if rule.keys.is_some() {
                    return Err(ConfigGuardError::Schema(format!(
                        "'keys' is only valid for type 'object', not 'string' {}",
                        context
                    )));
                }
                if rule.items.is_some() {
                    return Err(ConfigGuardError::Schema(format!(
                        "'items' is only valid for type 'list', not 'string' {}",
                        context
                    )));
                }
                if rule.min.is_some() || rule.max.is_some() {
                    return Err(ConfigGuardError::Schema(format!(
                        "'min' and 'max' are only valid for numeric types, not 'string' {}",
                        context
                    )));
                }

                // Validate min_length/max_length relationship if both are specified
                if let (Some(min), Some(max)) = (rule.min_length, rule.max_length) {
                    if min > max {
                        return Err(ConfigGuardError::Schema(format!(
                            "'min_length' cannot be greater than 'max_length' {}",
                            context
                        )));
                    }
                }

                // Validate regex pattern if specified
                if let Some(pattern) = &rule.pattern {
                    Regex::new(pattern).map_err(|e| {
                        ConfigGuardError::Pattern(format!(
                            "Invalid regex pattern '{}' {}: {}",
                            pattern, context, e
                        ))
                    })?;
                }
            }
            SchemaType::Integer | SchemaType::Float => {
                // Validate number-specific properties
                if rule.keys.is_some() {
                    return Err(ConfigGuardError::Schema(format!(
                        "'keys' is only valid for type 'object', not numeric type {}",
                        context
                    )));
                }
                if rule.items.is_some() {
                    return Err(ConfigGuardError::Schema(format!(
                        "'items' is only valid for type 'list', not numeric type {}",
                        context
                    )));
                }
                if rule.min_length.is_some() || rule.max_length.is_some() {
                    return Err(ConfigGuardError::Schema(
                        format!("'min_length' and 'max_length' are only valid for type 'string' or 'list', not numeric type {}", context)
                    ));
                }
                if rule.pattern.is_some() {
                    return Err(ConfigGuardError::Schema(format!(
                        "'pattern' is only valid for type 'string', not numeric type {}",
                        context
                    )));
                }

                // Validate min/max relationship if both are specified
                if let (Some(min), Some(max)) = (&rule.min, &rule.max) {
                    // Safely extract numeric values for comparison
                    let min_val = min.as_f64();
                    let max_val = max.as_f64();

                    if let (Some(min_num), Some(max_num)) = (min_val, max_val) {
                        if min_num > max_num {
                            return Err(ConfigGuardError::Schema(format!(
                                "'min' ({}) cannot be greater than 'max' ({}) {}",
                                min_num, max_num, context
                            )));
                        }
                    } else {
                        return Err(ConfigGuardError::Schema(format!(
                            "Non-numeric values used for 'min'/'max' in schema {} - min: {:?}, max: {:?}",
                            context, min, max
                        )));
                    }
                }
            }
            SchemaType::Boolean | SchemaType::Null => {
                // Validate boolean/null-specific properties - they shouldn't have most constraints
                if rule.keys.is_some() {
                    return Err(ConfigGuardError::Schema(format!(
                        "'keys' is only valid for type 'object' {}",
                        context
                    )));
                }
                if rule.items.is_some() {
                    return Err(ConfigGuardError::Schema(format!(
                        "'items' is only valid for type 'list' {}",
                        context
                    )));
                }
                if rule.min_length.is_some() || rule.max_length.is_some() {
                    return Err(ConfigGuardError::Schema(
                        format!("'min_length' and 'max_length' are only valid for type 'string' or 'list' {}", context)
                    ));
                }
                if rule.pattern.is_some() {
                    return Err(ConfigGuardError::Schema(format!(
                        "'pattern' is only valid for type 'string' {}",
                        context
                    )));
                }
                if rule.min.is_some() || rule.max.is_some() {
                    return Err(ConfigGuardError::Schema(format!(
                        "'min' and 'max' are only valid for numeric types {}",
                        context
                    )));
                }
            }
            SchemaType::Any => {
                // Any type has fewer restrictions, but should still not have type-specific constraints
                if rule.keys.is_some() {
                    return Err(ConfigGuardError::Schema(format!(
                        "'keys' is only valid for type 'object', not 'any' {}",
                        context
                    )));
                }
                if rule.items.is_some() {
                    return Err(ConfigGuardError::Schema(format!(
                        "'items' is only valid for type 'list', not 'any' {}",
                        context
                    )));
                }
                if rule.min_length.is_some() || rule.max_length.is_some() {
                    return Err(ConfigGuardError::Schema(
                        format!("'min_length' and 'max_length' are only valid for specific types, not 'any' {}", context)
                    ));
                }
                if rule.pattern.is_some() {
                    return Err(ConfigGuardError::Schema(format!(
                        "'pattern' is only valid for type 'string', not 'any' {}",
                        context
                    )));
                }
                if rule.min.is_some() || rule.max.is_some() {
                    return Err(ConfigGuardError::Schema(format!(
                        "'min' and 'max' are only valid for numeric types, not 'any' {}",
                        context
                    )));
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    // Helper to create a temporary schema file
    fn create_temp_schema_file(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file
    }

    #[test]
    fn test_load_valid_schema() {
        let schema_content = r#"
            type: object
            keys:
              name:
                type: string
                required: true
              count:
                type: integer
                min: 0
        "#;

        let schema_file = create_temp_schema_file(schema_content);
        let schema = Schema::from_file(schema_file.path()).unwrap();

        assert_eq!(schema.root.data_type, SchemaType::Object);
        assert!(schema.root.keys.is_some());
        let keys = schema.root.keys.as_ref().unwrap();
        assert!(keys.contains_key("name"));
        assert!(keys.contains_key("count"));

        let name_rule = keys.get("name").unwrap();
        assert_eq!(name_rule.data_type, SchemaType::String);
        assert!(name_rule.required);

        let count_rule = keys.get("count").unwrap();
        assert_eq!(count_rule.data_type, SchemaType::Integer);
        assert!(!count_rule.required); // Default is false
        assert!(count_rule.min.is_some());
    }

    #[test]
    fn test_invalid_schema_syntax() {
        let invalid_schema = "type: object\n  invalid-yaml-indentation";
        let schema_file = create_temp_schema_file(invalid_schema);
        let result = Schema::from_file(schema_file.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_schema_validation_invalid_type() {
        let schema_content = "type: invalidtype";
        let schema_file = create_temp_schema_file(schema_content);
        let result = Schema::from_file(schema_file.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_schema_validation_wrong_constraints() {
        // String with numeric constraints
        let schema_content = r#"
            type: string
            min: 5
            max: 10
        "#;
        let schema_file = create_temp_schema_file(schema_content);
        let result = Schema::from_file(schema_file.path());
        assert!(result.is_err());

        // Object with list constraints
        let schema_content = r#"
            type: object
            items:
              type: string
        "#;
        let schema_file = create_temp_schema_file(schema_content);
        let result = Schema::from_file(schema_file.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_schema_validation_invalid_regex() {
        let schema_content = r#"
            type: string
            pattern: "*[invalid regex"
        "#;
        let schema_file = create_temp_schema_file(schema_content);
        let result = Schema::from_file(schema_file.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_schema_nested_object() {
        let schema_content = r#"
            type: object
            keys:
              metadata:
                type: object
                keys:
                  name:
                    type: string
                  labels:
                    type: object
                    allow_unknown_keys: true
        "#;
        let schema_file = create_temp_schema_file(schema_content);
        let schema = Schema::from_file(schema_file.path()).unwrap();

        let keys = schema.root.keys.as_ref().unwrap();
        let metadata = keys.get("metadata").unwrap();
        assert_eq!(metadata.data_type, SchemaType::Object);

        let metadata_keys = metadata.keys.as_ref().unwrap();
        assert!(metadata_keys.contains_key("name"));
        assert!(metadata_keys.contains_key("labels"));

        let labels = metadata_keys.get("labels").unwrap();
        assert_eq!(labels.data_type, SchemaType::Object);
        assert!(labels.allow_unknown_keys);
    }
}
