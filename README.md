# Model Context Protocol Crates

## Crates

### `mcp-core`

Generalized traits an types that correctly implement the MCP protocol for consumption by Client and Server implementations.

#### Features

- Protocol implementation wrappable by Clients and Servers
- `Transport` trait
- Generic error types
- Convenience types for messages and error responses

### `mcp-types`

Rust type bindings for the [Model Context Protocol (MCP) specification](https://spec.modelcontextprotocol.io/specification/2024-11-05/), automatically generated from the protocol's [JSON schema](https://github.com/modelcontextprotocol/specification/blob/main/schema/2024-11-05/schema.json) using the `typify` crate.

#### Features

- Complete type definitions for the MCP protocol
- Serde serialization/deserialization support
- Generated from the latest MCP schema specification
- Type-safe interaction with MCP messages
- Schema validation test coverage

## ⚠️ Status: Alpha

The crates in this repo are currently in early development and are probably not ready for production use.

## Motivation

To provide auto-generated bindings for the MCP spec as well as generalized traits and types that correctly implement MCP for consumption by Client and Server developers.

## Contributing

Contributions are welcome! To contribute, please fork this repo, make a new branch off of `main`, and PR back to the `main` branch of this repo.

