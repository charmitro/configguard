# ConfigGuard

A high-performance validation tool for YAML and JSON configuration files, with a flexible schema definition system.

## Overview

ConfigGuard helps catch configuration errors early in development or CI/CD pipelines, preventing runtime failures caused by invalid structures, types, or values. It emphasizes:

- **Accuracy**: Reliable validation according to defined schema rules
- **Performance**: Quick execution for CI/CD pipelines
- **Usability**: Simple CLI interface with detailed error messages
- **Standalone**: Distributed as a single binary with no external dependencies
- **Flexibility**: Support for complex nested structures and precise constraints

## Installation

### From Source

```bash
git clone https://github.com/charmitro/configguard
cd configguard
cargo build --release
```

The binary will be located at `target/release/configguard`.

You can also install this via `cargo install`:

```bash
cargo install --path .
```

Or directly from crates.io:

```bash
cargo install configguard
```

## Usage

### Basic Validation

```bash
configguard validate config.yaml --schema schema.yaml
```

ConfigGuard automatically detects YAML (.yaml, .yml) and JSON (.json) configuration files based on their extension.

### JSON Output Format

```bash
configguard validate config.yaml --schema schema.yaml --format json
```

### Strict Mode (Reject Unknown Keys)

```bash
configguard validate config.yaml --schema schema.yaml --strict
```

### Validate Multiple Files

```bash
configguard validate config1.yaml config2.yaml --schema schema.yaml
```

### Validate All Files in a Directory

```bash
configguard validate ./configs/ --schema schema.yaml --directory
```

### Exit Codes

ConfigGuard uses the following exit codes:

- `0`: Success - All configurations are valid
- `2`: File not found
- `3`: File read/write error
- `4`: Parse error (invalid YAML/JSON)
- `5`: Unsupported file format
- `10`: Validation error(s)
- `11`: Schema error
- `12`: Pattern error (invalid regex)
- `20`: CLI error

## Options

- `--schema, -s <path>`: Path to the schema definition file (required)
- `--format <type>`: Output format (`text` (default), `json`)
- `--strict`: Enable strict validation (reject unknown fields)
- `--directory, -d`: Process all compatible files in specified directories

## Schema Definition

ConfigGuard uses a YAML-based schema definition language. Here's an example:

```yaml
type: object
description: Root configuration object
keys:
  apiVersion:
    type: string
    required: true
    pattern: ^v1(alpha|beta)?\d*$
    description: The API version string.
  kind:
    type: string
    required: true
    enum: [Deployment, Service, ConfigMap]
    description: The type of Kubernetes resource.
  metadata:
    type: object
    required: true
    keys:
      name:
        type: string
        required: true
        min_length: 1
        max_length: 63
      labels:
        type: object
        allow_unknown_keys: true # Allow arbitrary labels
  spec:
    type: object
    required: true
    keys:
      replicas:
        type: integer
        min: 0
        description: Number of replicas.
      containers:
        type: list
        required: true
        min_length: 1
        items:
          type: object
          keys:
            name: { type: string, required: true }
            image: { type: string, required: true }
```

See the `examples/` directory for more schema examples.

### Supported Types

- `string`: Text values
- `integer`: Whole numbers
- `float`: Decimal numbers
- `boolean`: True/false values
- `object`: Nested structures with key-value pairs
- `list`: Ordered collections of items

### Type-Specific Constraints

#### Common
- `description`: Human-readable description of the field (shown in error messages)
- `required`: Whether the key must exist (defaults to false)

#### Object Type
- `keys`: Map of child keys and their validation rules
- `allow_unknown_keys`: Whether to allow keys not defined in schema (defaults to false in strict mode)

#### List Type
- `items`: Validation rules applied to each list item
- `min_length`: Minimum number of items
- `max_length`: Maximum number of items

#### String Type
- `pattern`: Regular expression the string must match
- `enum`: List of allowed values
- `min_length`: Minimum string length
- `max_length`: Maximum string length

#### Numeric Types (Integer/Float)
- `min`: Minimum allowed value (inclusive)
- `max`: Maximum allowed value (inclusive)
- `enum`: List of allowed values

## Error Reporting

### Text Format

ConfigGuard provides detailed error messages showing exactly what's wrong with your configuration:

```
Error: 7 validation errors found:
1. Error at path '.apiVersion': String doesn't match pattern
   Expected: Pattern: ^v1(alpha|beta)?\d*$
   Found: v2
   Description: The API version string.

2. Error at path '.kind': Value not in allowed set
   Expected: One of: String("Deployment"), String("Service"), String("ConfigMap")
   Found: Job
   Description: The type of Kubernetes resource.

3. Error at path '.metadata.name': String too long
   Expected: At most 63 characters
   Found: 79 characters
```

### JSON Format

For integration with other tools, use JSON output format:

```json
{
  "valid": false,
  "error_count": 7,
  "errors": [
    {
      "path": ".apiVersion",
      "message": "String doesn't match pattern",
      "expected": "Pattern: ^v1(alpha|beta)?\\d*$",
      "actual": "v2",
      "description": "The API version string."
    },
    {
      "path": ".kind",
      "message": "Value not in allowed set",
      "expected": "One of: String(\"Deployment\"), String(\"Service\"), String(\"ConfigMap\")",
      "actual": "Job",
      "description": "The type of Kubernetes resource."
    }
  ]
}
```

## Examples

The `examples/` directory contains sample configurations and schemas to help you get started:

- `simple/`: Basic examples for validating individual config files
- `directory-validation/`: Examples for validating multiple files in a directory

## License

This project is licensed under the MIT License - see the LICENSE file for details.
