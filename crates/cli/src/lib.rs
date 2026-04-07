#![expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "CLI binary produces intentional terminal output"
)]

/// CODEOWNERS file parser and ownership lookup.
pub mod codeowners;

/// Structured error output for CLI and JSON formats.
pub mod error;

/// Metric and rule definitions for explainable CLI output.
pub mod explain;

/// Health / complexity analysis report types.
pub mod health_types;

/// Regression detection: baseline comparison and tolerance checking.
pub mod regression;

/// Report formatting utilities for analysis results.
///
/// Exposed for snapshot testing of output formats.
pub mod report;
