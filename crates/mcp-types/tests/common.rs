use serde_json::Value;
use std::fs;

pub fn get_schema_validator() -> jsonschema::Validator {
    let schema_str =
        fs::read_to_string("spec/2024-11-05-schema.json").expect("Failed to read schema file");

    let schema: Value = serde_json::from_str(&schema_str).expect("Failed to parse schema JSON");

    jsonschema::validator_for(&schema).expect("Failed to compile JSON schema")
}
