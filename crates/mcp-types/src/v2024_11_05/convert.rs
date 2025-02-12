use jsonschema::Validator;
use once_cell::sync::Lazy;
use serde::de::DeserializeOwned;
use serde_json::Value;

static SCHEMA: Lazy<String> =
    Lazy::new(|| include_str!("../../spec/2024-11-05-schema.json").to_string());

/// Try to infer and deserialize a specific type from a JSON value
///
/// Warning: Some types generated don't match their schema name, but they aren't ones that need to be deserialized directly usually.
/// For example, [`crate::ServerCapabilitiesTools`] doesn't match the schema name "Tools", so when it's type name is used for the schema
/// reference, it will fail to find the schema.
pub fn assert_v2024_11_05_type<T>(value: Value) -> Option<T>
where
    T: DeserializeOwned,
{
    // 1. Get the fully qualified type name, for example "my_crate::module::MyType".
    let full_type_name = std::any::type_name::<T>();

    // 2. Assume the last segment is the type name (i.e. "MyType").
    let type_name = full_type_name.rsplit("::").next().unwrap();
    let schema_ref = format!("#/definitions/{}", type_name);

    // 3. Parse the full JSON Schema.
    let schema_json: Value = serde_json::from_str(SCHEMA.as_ref()).ok()?;

    // 4. Convert the schema ref into a JSON pointer.
    //    A schema reference like "#/definitions/MyType" becomes the pointer "/definitions/MyType".
    let pointer = &schema_ref[1..]; // strip the leading '#'
    let sub_schema = schema_json.pointer(pointer)?;

    println!("sub_schema: {}", sub_schema);

    // 5. Compile the subschema.
    let schema_validator = Validator::new(sub_schema).ok()?;

    println!("schema_validator: {:?}", schema_validator);

    // 6. Validate the value against the compiled schema.
    if !schema_validator.is_valid(&value) {
        return None;
    }

    // 7. If validation passes, deserialize the value into T.
    serde_json::from_value(value).ok()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde_json::Map;

    use super::*;
    use crate::{
        v2024_11_05::types::{LoggingLevel, ServerCapabilitiesTools, Tool, ToolInputSchema},
        ServerCapabilities,
    };

    #[test]
    fn test_assert_type() {
        // Test enum deserialization
        let logging_level_json = serde_json::json!("info");
        let logging_level = assert_v2024_11_05_type::<LoggingLevel>(logging_level_json);
        assert_eq!(logging_level, Some(LoggingLevel::Info));

        // Test simple object deserialization
        let capabilities = ServerCapabilities {
            experimental: HashMap::default(),
            logging: Map::default(),
            prompts: Some(crate::ServerCapabilitiesPrompts {
                list_changed: Some(true),
            }),
            resources: Some(crate::ServerCapabilitiesResources {
                list_changed: Some(true),
                subscribe: Some(false),
            }),
            tools: Some(ServerCapabilitiesTools {
                list_changed: Some(false),
            }),
        };
        let capabilities_json = serde_json::to_value(capabilities.clone()).unwrap();
        let actual_capabilities = assert_v2024_11_05_type::<ServerCapabilities>(capabilities_json);
        assert_eq!(actual_capabilities, Some(capabilities));

        // Test complex object deserialization
        let tool_json = serde_json::json!({
            "name": "test_tool",
            "description": "A test tool",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "required": []
            }
        });
        let tool = assert_v2024_11_05_type::<Tool>(tool_json);
        assert_eq!(
            tool,
            Some(Tool {
                name: "test_tool".to_string(),
                description: Some("A test tool".to_string()),
                input_schema: ToolInputSchema {
                    type_: "object".to_string(),
                    properties: Default::default(),
                    required: vec![],
                }
            })
        );

        // Test invalid type
        let invalid_json = serde_json::json!({
            "name": 123  // name should be a string
        });
        let invalid_tool = assert_v2024_11_05_type::<Tool>(invalid_json);
        assert_eq!(invalid_tool, None);
    }
}
