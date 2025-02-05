#![cfg(feature = "client")]
use std::{
    io::{BufRead, BufReader, Write},
    process::Child,
};

use mcp_core::transport::{ReceiveTransport, Transport};
use mcp_types::JSONRPCMessage;
use tracing::info;

use crate::params::StdioServerParams;

pub struct StdioClientTransport {
    server_params: StdioServerParams,
    child: Option<Child>,
    stdin_handle: Option<std::process::ChildStdin>,
    stdout_handle: Option<std::process::ChildStdout>,
    stderr_handle: Option<std::process::ChildStderr>,
    started: bool,
}

impl StdioClientTransport {
    pub fn new(server_params: StdioServerParams) -> Self {
        Self {
            server_params,
            child: None,
            stdin_handle: None,
            stdout_handle: None,
            stderr_handle: None,
            started: false,
        }
    }

    fn get_inherit_envs(&self) -> Vec<(String, String)> {
        let keys = if self.server_params.inherit_envs.is_none() {
            let env = if cfg!(target_os = "windows") {
                vec![
                    "APPDATA",
                    "HOMEDRIVE",
                    "HOMEPATH",
                    "LOCALAPPDATA",
                    "PATH",
                    "PROCESSOR_ARCHITECTURE",
                    "SYSTEMDRIVE",
                    "SYSTEMROOT",
                    "TEMP",
                    "USERNAME",
                    "USERPROFILE",
                ]
            } else {
                vec!["HOME", "LOGNAME", "PATH", "SHELL", "TERM", "USER"]
            };

            env.iter().map(|s| s.to_string()).collect()
        } else {
            self.server_params.inherit_envs.clone().unwrap()
        };

        let mut inherit_envs: Vec<(String, String)> = Vec::new();
        for key in keys {
            let var = match std::env::var(&key) {
                Ok(var) => var,
                Err(_) => continue,
            };

            inherit_envs.push((key, var));
        }

        inherit_envs
    }

    #[cfg(unix)]
    pub fn graceful_shutdown(&mut self) -> Result<(), std::io::Error> {
        use nix::sys::signal::{self, Signal};
        use nix::unistd::Pid;
        use tracing::info;

        if !self.started {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "transport not started",
            ));
        }

        let child = match self.child.as_mut() {
            Some(child) => child,
            None => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "expected child process is missing",
                ))
            }
        };

        // Send SIGTERM to child process
        signal::kill(Pid::from_raw(child.id() as i32), Signal::SIGTERM)?;

        // Wait for child to exit
        let now = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(5);

        while now.elapsed() < timeout {
            if let Some(_status) = child.try_wait()? {
                // try_wait doesn't do clean up so we call wait() to handle it
                let status = child.wait()?;

                info!("child process exited with status: {status}");

                self.child = None;

                return Ok(());
            }
        }

        Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "timed out waiting for child process to exit",
        ))
    }

    #[cfg(windows)]
    pub fn graceful_shutdown(&mut self) -> Result<(), std::io::Error> {
        todo!()
    }

    #[cfg(unix)]
    pub fn kill_child(&mut self) -> Result<(), std::io::Error> {
        let child = match self.child.as_mut() {
            Some(child) => child,
            None => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "expected child process is missing",
                ))
            }
        };

        child.kill()?;

        Ok(())
    }

    #[cfg(windows)]
    pub fn kill_child(&mut self) -> Result<(), std::io::Error> {
        todo!()
    }
}

#[async_trait::async_trait]
impl Transport for StdioClientTransport {
    type Error = std::io::Error;

    // TODO: Find out if BufReader obtains a lock on stdio on construction or only at read/write time.
    async fn start(&mut self) -> Result<(), Self::Error> {
        if self.started {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "transport already started",
            ));
        }

        let command = self.server_params.command.clone();
        let mut command = std::process::Command::new(&command);

        // Set args
        let args = self.server_params.args.clone();

        command.args(&args);

        // Clear all but configured env vars, inherit the rest
        let inherit_envs = self.get_inherit_envs();

        command.env_clear();
        command.envs(inherit_envs);

        // Configure streams
        command.stdout(std::process::Stdio::piped());
        command.stdin(std::process::Stdio::piped());
        command.stderr(std::process::Stdio::piped());

        // Start the server
        self.child = Some(command.spawn()?);
        self.stdin_handle = self.child.as_mut().unwrap().stdin.take();
        self.stdout_handle = self.child.as_mut().unwrap().stdout.take();
        self.stderr_handle = self.child.as_mut().unwrap().stderr.take();

        Ok(())
    }

    async fn close(&mut self) -> Result<(), Self::Error> {
        if !self.started {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "transport not started",
            ));
        }

        if let Err(err) = self.graceful_shutdown() {
            tracing::warn!("failed to gracefully shutdown child process: {}", err);
            tracing::info!("killing child process");

            if let Err(err) = self.kill_child() {
                tracing::error!("failed to kill child process: {err}");

                let pid = self.child.as_ref().unwrap().id();
                tracing::warn!("child process might be zombied. manually kill it with pid: {pid}");
            }
        }

        self.child = None;
        self.stdin_handle = None;
        self.stdout_handle = None;
        self.stderr_handle = None;

        Ok(())
    }

    async fn send(&mut self, message: JSONRPCMessage) -> Result<(), Self::Error> {
        if !self.started {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "transport not started",
            ));
        }

        let message = serde_json::to_string(&message).unwrap();
        let stdin = self.stdin_handle.as_mut().unwrap();
        let mut writer = std::io::BufWriter::new(stdin);

        writer.write_all(message.as_bytes())?;
        writer.flush()?;

        Ok(())
    }
}

#[async_trait::async_trait]
impl ReceiveTransport for StdioClientTransport {
    async fn receive(&mut self) -> Result<JSONRPCMessage, Self::Error> {
        if !self.started {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "transport not started",
            ));
        }

        let stdout = self.stdout_handle.as_mut().unwrap();
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        reader.read_line(&mut line)?;

        match serde_json::from_str(&line) {
            Ok(message) => Ok(message),
            Err(err) => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("failed to parse message: {err}"),
            )),
        }
    }
}
