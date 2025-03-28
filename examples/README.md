# ConfigGuard Examples

This directory contains example files demonstrating how to use ConfigGuard for different validation scenarios.

## Directory Structure

- **simple/** - Basic examples for validating individual config files
  - `schema.yaml` - Schema definition
  - `valid-config.yaml` - Valid YAML configuration
  - `valid-config.json` - Valid JSON configuration
  - `invalid-config.yaml` - YAML with validation errors
  - `invalid-config.json` - JSON with validation errors

- **directory-validation/** - Examples for validating multiple files in a directory
  - `schema.yaml` - Schema definition
  - `service1.yaml` - Valid service configuration (YAML)
  - `service2.json` - Valid service configuration (JSON)
  - `invalid-deployment.yaml` - Invalid deployment configuration
  - `unrelated.txt` - Non-configuration file (should be skipped)

## Quick Start

### Validate a Single Configuration

```bash
# Validate a YAML configuration
configguard validate simple/valid-config.yaml --schema simple/schema.yaml

# Validate a JSON configuration
configguard validate simple/valid-config.json --schema simple/schema.yaml

# Validate with JSON output
configguard validate simple/valid-config.yaml --schema simple/schema.yaml --format json

# Validate in strict mode
configguard validate simple/valid-config.yaml --schema simple/schema.yaml --strict
```

### Validate Multiple Configurations in a Directory

```bash
# Validate all YAML and JSON files in a directory
configguard validate directory-validation/ --schema directory-validation/schema.yaml --directory
```