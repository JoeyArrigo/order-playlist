//! CLI presentation — clap argument structs, ASCII chart, summary
//! report formatter.
//!
//! This module owns terminal output. The `main.rs` binary is the only
//! other place permitted to call `println!`/`eprintln!` directly.

pub mod args;
pub mod chart;
pub mod report;

pub use args::{Args, ResolvedArgs};
pub use chart::render_arc;
pub use report::format_summary;
