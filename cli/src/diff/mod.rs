//! `starbreaker diff` subcommands for comparing game builds.
//!
//! - `inventory` / `compare` (`compare.rs`): hash-based P4k + DataCore
//!   inventories and their structured comparison.
//! - `report` (`report.rs`): a text report tree meant to be diffed with
//!   external tools (git, diff viewers).

mod compare;
mod report;

use clap::Subcommand;

use crate::error::Result;

#[derive(Subcommand)]
pub enum DiffCommand {
    /// Generate a reusable P4k/DataCore inventory report.
    Inventory(compare::InventoryArgs),
    /// Compare two P4k files and/or inventory reports.
    Compare(compare::CompareArgs),
    /// Write a git-diffable report tree (P4k listing, contents, DataCore XML/JSON).
    Report(report::ReportArgs),
}

impl DiffCommand {
    pub fn run(self) -> Result<()> {
        match self {
            Self::Inventory(args) => compare::inventory(args),
            Self::Compare(args) => compare::compare(args),
            Self::Report(args) => args.run(),
        }
    }
}
