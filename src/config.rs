use crate::error::{ConfigGuardError, ConfigGuardResult};
use serde_yaml::Value;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

/// Supported configuration file formats
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConfigFormat {
    /// YAML format (.yaml, .yml)
    Yaml,
    /// JSON format (.json)
    Json,
}

/// Represents a configuration to be validated
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Config {
    pub data: Value,
    pub format: ConfigFormat,
    pub path: Option<PathBuf>,
}

impl Config {
    /// Create a new configuration from raw content
    pub fn from_str(content: &str, format: ConfigFormat) -> ConfigGuardResult<Self> {
        let data = match format {
            ConfigFormat::Yaml => serde_yaml::from_str(content)
                .map_err(|e| ConfigGuardError::ParseYaml(e.to_string()))?,
            ConfigFormat::Json => serde_json::from_str(content)
                .map_err(|e| ConfigGuardError::ParseJson(e.to_string()))?,
        };

        Ok(Self {
            data,
            format,
            path: None,
        })
    }

    /// Load a configuration from a file
    pub fn from_file<P: AsRef<Path>>(path: P) -> ConfigGuardResult<Self> {
        let path_ref = path.as_ref();
        let format = detect_format(path_ref)?;

        let content = fs::read_to_string(path_ref).map_err(|e| ConfigGuardError::FileRead {
            path: path_ref.display().to_string(),
            error: e.to_string(),
        })?;

        let mut config = Self::from_str(&content, format)?;
        config.path = Some(path_ref.to_path_buf());

        Ok(config)
    }
}

/// Detect the format of a configuration file based on its extension
fn detect_format<P: AsRef<Path>>(path: P) -> ConfigGuardResult<ConfigFormat> {
    let extension = path
        .as_ref()
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_lowercase());

    match extension.as_deref() {
        Some("yaml") | Some("yml") => Ok(ConfigFormat::Yaml),
        Some("json") => Ok(ConfigFormat::Json),
        _ => Err(ConfigGuardError::UnsupportedFormat {
            path: path.as_ref().display().to_string(),
            extension: extension.unwrap_or_default(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    // Helper to create a temporary file with content
    fn create_temp_file(content: &str, extension: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        // Rename with the desired extension
        let path = file.path().to_owned();
        let new_path = path.with_extension(extension);
        std::fs::rename(&path, &new_path).unwrap();

        // This is a bit hacky but allows us to get a temp file with a specific extension
        file
    }

    #[test]
    fn test_detect_format() {
        // Valid YAML extensions
        assert_eq!(detect_format("config.yaml").unwrap(), ConfigFormat::Yaml);
        assert_eq!(detect_format("config.yml").unwrap(), ConfigFormat::Yaml);

        // Valid JSON extension
        assert_eq!(detect_format("config.json").unwrap(), ConfigFormat::Json);

        // Invalid extension
        let result = detect_format("config.txt");
        assert!(result.is_err());
        match result {
            Err(ConfigGuardError::UnsupportedFormat { path, extension }) => {
                assert_eq!(path, "config.txt");
                assert_eq!(extension, "txt");
            }
            _ => panic!("Expected UnsupportedFormat error"),
        }
    }

    #[test]
    fn test_load_valid_yaml() {
        let yaml = r#"
            name: test
            spec:
              replicas: 3
              containers:
                - name: app
                  image: nginx:latest
        "#;

        let config = Config::from_str(yaml, ConfigFormat::Yaml).unwrap();
        assert_eq!(config.format, ConfigFormat::Yaml);

        if let Value::Mapping(map) = &config.data {
            assert!(map.contains_key(&Value::String("name".to_string())));
            assert!(map.contains_key(&Value::String("spec".to_string())));
        } else {
            panic!("Expected mapping");
        }
    }

    #[test]
    fn test_load_valid_json() {
        let json = r#"{
            "name": "test",
            "spec": {
                "replicas": 3,
                "containers": [
                    {
                        "name": "app",
                        "image": "nginx:latest"
                    }
                ]
            }
        }"#;

        let config = Config::from_str(json, ConfigFormat::Json).unwrap();
        assert_eq!(config.format, ConfigFormat::Json);

        if let Value::Mapping(map) = &config.data {
            assert!(map.contains_key(&Value::String("name".to_string())));
            assert!(map.contains_key(&Value::String("spec".to_string())));
        } else {
            panic!("Expected mapping");
        }
    }

    #[test]
    fn test_load_invalid_yaml() {
        let invalid_yaml = "this: is: invalid: yaml:";
        let result = Config::from_str(invalid_yaml, ConfigFormat::Yaml);

        assert!(result.is_err());
        match result {
            Err(ConfigGuardError::ParseYaml(_)) => (),
            _ => panic!("Expected ParseYaml error"),
        }
    }

    #[test]
    fn test_load_invalid_json() {
        let invalid_json = "{name: test}";
        let result = Config::from_str(invalid_json, ConfigFormat::Json);

        assert!(result.is_err());
        match result {
            Err(ConfigGuardError::ParseJson(_)) => (),
            _ => panic!("Expected ParseJson error"),
        }
    }

    #[test]
    fn test_load_yaml_config() {
        let yaml_content = r#"
            name: test-app
            version: 1.0
            options:
              debug: true
              timeout: 30
        "#;

        let temp_file = create_temp_file(yaml_content, "yaml");
        let path = temp_file.path().with_extension("yaml");

        let config = Config::from_file(&path).unwrap();
        assert_eq!(config.format, ConfigFormat::Yaml);

        if let Value::Mapping(map) = &config.data {
            assert!(map.contains_key(&Value::String("name".to_string())));
            assert!(map.contains_key(&Value::String("version".to_string())));
            assert!(map.contains_key(&Value::String("options".to_string())));
        } else {
            panic!("Config data should be a mapping");
        }
    }

    #[test]
    fn test_load_json_config() {
        let json_content = r#"
        {
            "name": "test-app",
            "version": 1.0,
            "options": {
                "debug": true,
                "timeout": 30
            }
        }
        "#;

        let temp_file = create_temp_file(json_content, "json");
        let path = temp_file.path().with_extension("json");

        let config = Config::from_file(&path).unwrap();
        assert_eq!(config.format, ConfigFormat::Json);

        if let Value::Mapping(map) = &config.data {
            assert!(map.contains_key(&Value::String("name".to_string())));
            assert!(map.contains_key(&Value::String("version".to_string())));
            assert!(map.contains_key(&Value::String("options".to_string())));
        } else {
            panic!("Config data should be a mapping");
        }
    }

    #[test]
    fn test_unsupported_extension() {
        let config_content = "Some config content";
        let temp_file = create_temp_file(config_content, "txt");
        let path = temp_file.path().with_extension("txt");

        let result = Config::from_file(&path);
        assert!(result.is_err());
    }

    #[test]
    fn test_nonexistent_file() {
        let result = Config::from_file("nonexistent_file.yaml");
        assert!(result.is_err());
    }
}
