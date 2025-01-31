# mcp-types

Rust type bindings for the Model Context Protocol (MCP) specification, automatically generated from the protocol's JSON schema using the `typify` crate.

## Overview

This crate provides strongly-typed Rust structs and enums that represent the Model Context Protocol, a JSON-RPC based protocol for communication between LLM clients and servers. The types are automatically generated at build time from the official MCP JSON schema specification.

## ⚠️ Status: Alpha

This crate is currently in early development and is probably not ready for production use.

## Features

- Complete type definitions for the MCP protocol
- Serde serialization/deserialization support
- Generated from the latest MCP schema specification
- Type-safe interaction with MCP messages
- Schema validation test coverage

## Usage

Add this to your `Cargo.toml`:

```tomle
mcp-types = "0.1.0"
```

or run this from inside your crate directory tree:

```bash
cargo add mcp-types
```

## Contributing

Contributions are welcome! Please note that this crate's types are automatically generated from the MCP schema, so most changes will have to do with how the bindings are generated, how types are re-exported, documentation, and other things of that nature. Please do not manually edit generated code.

To contribute, please fork this repo, make a new branch off of `main`, and PR back to the `main` branch of this repo.

