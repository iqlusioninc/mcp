#![cfg(feature = "client")]

use serde::{Deserialize, Serialize};

/// Parameters for starting and configuring the Server child process
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StdioServerParams {
    /// The binary to run the server
    pub command: String,

    /// Any arguments to pass to the server
    pub args: Vec<String>,

    /// The server process environment variables to inherit.
    pub inherit_envs: Option<Vec<String>>,

    // The server process environment variables to set.
    pub envs: Option<Vec<String>>,
    // TODO: stderr behavior?
}
