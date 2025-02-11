use jsonschema::Validator;
use once_cell::sync::Lazy;
use serde_json::Value;
use std::collections::HashMap;

use super::types::*;

static SCHEMAS: Lazy<HashMap<&'static str, Validator>> = Lazy::new(|| {
    let schema_str = include_str!("../../spec/2024-11-05-schema.json");
    let schema: serde_json::Value = serde_json::from_str(schema_str).unwrap();
    let definitions = schema["definitions"].as_object().unwrap();

    let mut schemas = HashMap::new();

    // Create validators for each notification type
    let notification_types = [
        ("CancelledNotification", "CancelledNotification"),
        ("ProgressNotification", "ProgressNotification"),
        (
            "ResourceListChangedNotification",
            "ResourceListChangedNotification",
        ),
        ("ResourceUpdatedNotification", "ResourceUpdatedNotification"),
        (
            "PromptListChangedNotification",
            "PromptListChangedNotification",
        ),
        ("ToolListChangedNotification", "ToolListChangedNotification"),
        ("LoggingMessageNotification", "LoggingMessageNotification"),
        (
            "RootsListChangedNotification",
            "RootsListChangedNotification",
        ),
    ];

    for (name, def) in notification_types {
        let schema_obj = serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "$ref": format!("#/definitions/{}", def),
            "definitions": definitions,
        });
        schemas.insert(name, Validator::new(&schema_obj).unwrap());
    }

    schemas
});

/// Try to determine the specific notification type from a JSON value
pub fn infer_notification(value: &Value) -> Option<Box<dyn std::any::Any>> {
    for (name, validator) in SCHEMAS.iter() {
        if validator.is_valid(value) {
            return match *name {
                "CancelledNotification" => Some(Box::new(
                    serde_json::from_value::<CancelledNotification>(value.clone()).ok()?,
                )),
                "ProgressNotification" => Some(Box::new(
                    serde_json::from_value::<ProgressNotification>(value.clone()).ok()?,
                )),
                "ResourceListChangedNotification" => Some(Box::new(
                    serde_json::from_value::<ResourceListChangedNotification>(value.clone())
                        .ok()?,
                )),
                "ResourceUpdatedNotification" => Some(Box::new(
                    serde_json::from_value::<ResourceUpdatedNotification>(value.clone()).ok()?,
                )),
                "PromptListChangedNotification" => Some(Box::new(
                    serde_json::from_value::<PromptListChangedNotification>(value.clone()).ok()?,
                )),
                "ToolListChangedNotification" => Some(Box::new(
                    serde_json::from_value::<ToolListChangedNotification>(value.clone()).ok()?,
                )),
                "LoggingMessageNotification" => Some(Box::new(
                    serde_json::from_value::<LoggingMessageNotification>(value.clone()).ok()?,
                )),
                "RootsListChangedNotification" => Some(Box::new(
                    serde_json::from_value::<RootsListChangedNotification>(value.clone()).ok()?,
                )),
                _ => None,
            };
        }
    }
    None
}

// Helper function to downcast to a specific type
pub fn downcast<T: 'static>(boxed: Box<dyn std::any::Any>) -> Option<T> {
    boxed.downcast().ok().map(|b| *b)
}
