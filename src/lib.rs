//! Library root: re-exported modules so integration tests can reuse the
//! registry, state and dispatch without spawning the binary.

pub mod backend;
pub mod capture;
pub mod cli;
pub mod coords;
pub mod dispatch;
pub mod input;
pub mod mcp;
pub mod state;
pub mod tools;
