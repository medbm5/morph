//! Core of Metamorphosis: operation registry, type detection, naming,
//! atomic non-destructive output and the job runner.
//!
//! Engines implement [`Operation`]; the app and the `morph` CLI share one
//! [`Registry`], so anything the ring can do, the CLI can do.
pub mod components;
pub mod kind;
pub mod op;
pub mod output;
pub mod progress;
pub mod registry;

/// Version shared by the CLI and the app.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("unknown operation `{0}`")]
    UnknownOp(String),
    #[error("no input files")]
    NoInputs,
    #[error("`{op}` can't take {kind} files ({path})")]
    Unsupported { op: String, path: std::path::PathBuf, kind: String },
    #[error("needs the optional component `{0}`; install it from Settings or `morph component install {0}`")]
    ComponentMissing(String),
    #[error("{0}: {1}")]
    Io(std::path::PathBuf, std::io::Error),
    #[error("cancelled")]
    Cancelled,
}
