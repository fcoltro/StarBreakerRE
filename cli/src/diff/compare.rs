//! `diff inventory` / `diff compare`: build-level P4k + DataCore inventory
//! reports and their comparison (table, JSON or `.starbreaker-diff.json`
//! output), backed by the `starbreaker-diff` crate.

use std::collections::BTreeMap;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

use clap::{Parser, ValueEnum};
use starbreaker_diff::compare::{DiffItem, DiffSide, DiffStatus};
use starbreaker_diff::report::{
    extension_for_path, read_inventory_report, write_diff_report, write_inventory_report, InventoryReport, Tier,
    INVENTORY_EXTENSION,
};
use starbreaker_diff::{compare_reports, generate_inventory_from_p4k_with_progress, DiffFilter};

use crate::error::Result;

#[derive(Parser)]
pub struct InventoryArgs {
    /// Data.p4k path.
    source: PathBuf,
    /// Output *.starbreaker-inventory.json path.
    #[arg(short, long)]
    output: PathBuf,
    /// Skip DataCore record inventory and compare only P4k file entries.
    #[arg(long)]
    skip_datacore: bool,
    /// Optional display label stored in the report.
    #[arg(long)]
    label: Option<String>,
}

#[derive(Parser)]
pub struct CompareArgs {
    /// Old source: Data.p4k or *.starbreaker-inventory.json.
    old: PathBuf,
    /// New source: Data.p4k or *.starbreaker-inventory.json.
    new: PathBuf,
    /// Optional output *.starbreaker-diff.json path.
    #[arg(short, long)]
    output: Option<PathBuf>,
    /// Output format when not writing a diff file.
    #[arg(long, value_enum, default_value_t = OutputFormat::Table)]
    format: OutputFormat,
    /// Filter by tier.
    #[arg(long, value_enum, default_value_t = TierArg::All)]
    tier: TierArg,
    /// Filter by statuses. Comma-separated values are accepted.
    #[arg(long, value_delimiter = ',')]
    status: Vec<StatusArg>,
    /// Plain-text search across paths, names, types, extensions, and GUIDs.
    #[arg(long)]
    search: Option<String>,
    /// Filter archive files by extension, such as .dds or xml.
    #[arg(long, value_delimiter = ',')]
    extension: Vec<String>,
    /// Filter DataCore records by record type.
    #[arg(long = "record-type", value_delimiter = ',')]
    record_type: Vec<String>,
    /// Filter archive/DataCore paths by prefix.
    #[arg(long = "path-prefix", value_delimiter = ',')]
    path_prefix: Vec<String>,
    /// Include unchanged rows in the diff output.
    #[arg(long)]
    include_unchanged: bool,
    /// Skip DataCore if a compare input is a raw P4k.
    #[arg(long)]
    skip_datacore: bool,
    /// Maximum rows printed by table output. Use 0 for all rows.
    #[arg(long, default_value_t = 200)]
    limit: usize,
    /// Maximum groups printed in summary sections.
    #[arg(long = "summary-top", default_value_t = 20)]
    summary_top: usize,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum OutputFormat {
    Table,
    Summary,
    Json,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum TierArg {
    All,
    P4k,
    Datacore,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum StatusArg {
    Added,
    Removed,
    Modified,
    Metadata,
    Unchanged,
}

pub(super) fn inventory(args: InventoryArgs) -> Result<()> {
    let options = starbreaker_diff::InventoryOptions {
        skip_datacore: args.skip_datacore,
        label: args.label,
        ..Default::default()
    };
    let mut progress_output = CliProgress::new();
    let mut progress = |event: starbreaker_diff::ProgressEvent| progress_output.report(event);
    let report = generate_inventory_from_p4k_with_progress(
        &args.source,
        &options,
        Some(&mut progress),
        None,
    )?;
    drop(progress);
    progress_output.finish();
    write_inventory_report(&args.output, &report)?;
    eprintln!(
        "Wrote inventory: {} ({}, {} P4k entries, {} DataCore records)",
        args.output.display(),
        report.source.label,
        report.archive.len(),
        report.datacore.status.records().len()
    );
    Ok(())
}

pub(super) fn compare(args: CompareArgs) -> Result<()> {
    let options = starbreaker_diff::InventoryOptions {
        skip_datacore: args.skip_datacore,
        ..Default::default()
    };
    let old = load_source(&args.old, &options)?;
    let new = load_source(&args.new, &options)?;
    let mut diff = compare_reports(&old, &new, args.include_unchanged);
    let filter = build_filter(&args);
    if filter_has_constraints(&filter) {
        let filtered = starbreaker_diff::filter_diff_items(&diff.items, &filter);
        diff.items = filtered.into_iter().cloned().collect();
        diff.summary = summary_for_items(&diff.items);
    }

    if let Some(output) = args.output {
        write_diff_report(&output, &diff)?;
        eprintln!("Wrote diff: {}", output.display());
        return Ok(());
    }

    match args.format {
        OutputFormat::Table => print_table(&diff.items, args.limit),
        OutputFormat::Summary => print_summary(&diff, args.summary_top),
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&diff)?),
    }
    Ok(())
}

fn load_source(path: &Path, options: &starbreaker_diff::InventoryOptions) -> Result<InventoryReport> {
    if is_inventory_report(path) {
        return Ok(read_inventory_report(path)?);
    }
    let mut progress_output = CliProgress::new();
    let mut progress = |event: starbreaker_diff::ProgressEvent| progress_output.report(event);
    let report = generate_inventory_from_p4k_with_progress(
        path,
        options,
        Some(&mut progress),
        None,
    )?;
    drop(progress);
    progress_output.finish();
    Ok(report)
}

fn is_inventory_report(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(INVENTORY_EXTENSION))
}

fn build_filter(args: &CompareArgs) -> DiffFilter {
    let tiers = match args.tier {
        TierArg::All => Vec::new(),
        TierArg::P4k => vec![Tier::P4k],
        TierArg::Datacore => vec![Tier::DataCore],
    };
    let statuses = args.status.iter().map(|status| match status {
        StatusArg::Added => DiffStatus::Added,
        StatusArg::Removed => DiffStatus::Removed,
        StatusArg::Modified => DiffStatus::Modified,
        StatusArg::Metadata => DiffStatus::MetadataChanged,
        StatusArg::Unchanged => DiffStatus::Unchanged,
    }).collect();
    DiffFilter {
        search: args.search.clone(),
        tiers,
        statuses,
        extensions: args.extension.clone(),
        record_types: args.record_type.clone(),
        path_prefixes: args.path_prefix.clone(),
        include_unchanged: args.include_unchanged,
    }
}

fn filter_has_constraints(filter: &DiffFilter) -> bool {
    filter.search.as_ref().is_some_and(|s| !s.trim().is_empty())
        || !filter.tiers.is_empty()
        || !filter.statuses.is_empty()
        || !filter.extensions.is_empty()
        || !filter.record_types.is_empty()
        || !filter.path_prefixes.is_empty()
}

fn summary_for_items(items: &[DiffItem]) -> starbreaker_diff::DiffSummary {
    let mut summary = starbreaker_diff::DiffSummary::default();
    for item in items {
        match item.status {
            DiffStatus::Added => summary.added += 1,
            DiffStatus::Removed => summary.removed += 1,
            DiffStatus::Modified => summary.modified += 1,
            DiffStatus::MetadataChanged => summary.metadata_changed += 1,
            DiffStatus::Unchanged => summary.unchanged += 1,
        }
        match item.tier {
            Tier::P4k => summary.p4k_items += 1,
            Tier::DataCore => summary.datacore_items += 1,
        }
    }
    summary
}

fn print_table(items: &[DiffItem], limit: usize) {
    let shown = if limit == 0 { items.len() } else { items.len().min(limit) };
    println!("{:<16} {:<10} {:<32} Item", "Status", "Tier", "Reasons");
    println!("{:-<16} {:-<10} {:-<32} {:-<24}", "", "", "", "");
    for item in items.iter().take(shown) {
        println!(
            "{:<16} {:<10} {:<32} {}",
            status_label(item.status),
            tier_label(item.tier),
            item.reasons.join(","),
            item.display
        );
    }
    if shown < items.len() {
        eprintln!(
            "Showing {shown} of {} rows. Use --limit 0 for all rows, or --format summary for grouped totals.",
            items.len()
        );
    }
}

fn print_summary(diff: &starbreaker_diff::DiffReport, top: usize) {
    println!("{} -> {}", diff.old_label, diff.new_label);
    println!();
    println!("Totals");
    println!("  added:     {}", diff.summary.added);
    println!("  removed:   {}", diff.summary.removed);
    println!("  modified:  {}", diff.summary.modified);
    println!("  metadata:  {}", diff.summary.metadata_changed);
    if diff.summary.unchanged > 0 {
        println!("  unchanged: {}", diff.summary.unchanged);
    }
    println!("  p4k:       {}", diff.summary.p4k_items);
    println!("  datacore:  {}", diff.summary.datacore_items);

    print_status_by_tier(diff);
    print_group_counts("P4k file types", p4k_extension_counts(&diff.items), top);
    print_group_counts("DataCore record types", datacore_record_type_counts(&diff.items), top);
    print_group_counts("Change reasons", reason_counts(&diff.items), top);
}

fn status_label(status: DiffStatus) -> &'static str {
    match status {
        DiffStatus::Added => "added",
        DiffStatus::Removed => "removed",
        DiffStatus::Modified => "modified",
        DiffStatus::MetadataChanged => "metadata",
        DiffStatus::Unchanged => "unchanged",
    }
}

fn tier_label(tier: Tier) -> &'static str {
    match tier {
        Tier::P4k => "p4k",
        Tier::DataCore => "datacore",
    }
}

fn print_status_by_tier(diff: &starbreaker_diff::DiffReport) {
    println!();
    println!("By tier");
    println!("{:<10} {:>8} {:>8} {:>8} {:>10} {:>10}", "Tier", "Added", "Removed", "Modified", "Metadata", "Unchanged");
    println!("{:-<10} {:->8} {:->8} {:->8} {:->10} {:->10}", "", "", "", "", "", "");
    for tier in [Tier::P4k, Tier::DataCore] {
        println!(
            "{:<10} {:>8} {:>8} {:>8} {:>10} {:>10}",
            tier_label(tier),
            count_status(&diff.items, tier, DiffStatus::Added),
            count_status(&diff.items, tier, DiffStatus::Removed),
            count_status(&diff.items, tier, DiffStatus::Modified),
            count_status(&diff.items, tier, DiffStatus::MetadataChanged),
            count_status(&diff.items, tier, DiffStatus::Unchanged),
        );
    }
}

fn count_status(items: &[DiffItem], tier: Tier, status: DiffStatus) -> usize {
    items
        .iter()
        .filter(|item| item.tier == tier && item.status == status)
        .count()
}

fn p4k_extension_counts(items: &[DiffItem]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for item in items.iter().filter(|item| item.tier == Tier::P4k && item.status != DiffStatus::Unchanged) {
        let path = match item.new.as_ref().or(item.old.as_ref()) {
            Some(DiffSide::Archive(entry)) => &entry.path,
            _ => &item.display,
        };
        let extension = extension_for_path(path).unwrap_or_else(|| "(no extension)".to_string());
        *counts.entry(extension).or_default() += 1;
    }
    counts
}

fn datacore_record_type_counts(items: &[DiffItem]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for item in items.iter().filter(|item| item.tier == Tier::DataCore && item.status != DiffStatus::Unchanged) {
        let record_type = match item.new.as_ref().or(item.old.as_ref()) {
            Some(DiffSide::DataCore(record)) => record.record_type.as_str(),
            _ => "(unknown)",
        };
        *counts.entry(record_type.to_string()).or_default() += 1;
    }
    counts
}

fn reason_counts(items: &[DiffItem]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for item in items.iter().filter(|item| item.status != DiffStatus::Unchanged) {
        if item.reasons.is_empty() {
            *counts.entry(status_label(item.status).to_string()).or_default() += 1;
            continue;
        }
        for reason in &item.reasons {
            *counts.entry(reason.clone()).or_default() += 1;
        }
    }
    counts
}

fn print_group_counts(title: &str, counts: BTreeMap<String, usize>, top: usize) {
    if counts.is_empty() {
        return;
    }
    let mut rows: Vec<_> = counts.into_iter().collect();
    rows.sort_by(|(a_name, a_count), (b_name, b_count)| b_count.cmp(a_count).then_with(|| a_name.cmp(b_name)));
    let shown = if top == 0 { rows.len() } else { rows.len().min(top) };
    println!();
    println!("{title}");
    for (name, count) in rows.iter().take(shown) {
        println!("  {:>8}  {}", count, name);
    }
    if shown < rows.len() {
        println!("  ... {} more groups hidden; use --summary-top 0 to show all.", rows.len() - shown);
    }
}

struct CliProgress {
    dynamic: bool,
    line_active: bool,
}

impl CliProgress {
    fn new() -> Self {
        Self {
            dynamic: io::stderr().is_terminal(),
            line_active: false,
        }
    }

    fn report(&mut self, event: starbreaker_diff::ProgressEvent) {
        if self.dynamic && event.phase == starbreaker_diff::ProgressPhase::HashingDataCoreRecords {
            self.write_dynamic(&format_progress(&event));
            return;
        }

        self.finish();
        eprintln!("{}", format_progress(&event));
    }

    fn write_dynamic(&mut self, text: &str) {
        let mut stderr = io::stderr().lock();
        let _ = write!(stderr, "\r\x1b[2K{text}");
        let _ = stderr.flush();
        self.line_active = true;
    }

    fn finish(&mut self) {
        if self.line_active {
            eprintln!();
            self.line_active = false;
        }
    }
}

fn format_progress(event: &starbreaker_diff::ProgressEvent) -> String {
    match (event.current, event.total) {
        (Some(current), Some(total)) => format!("{}: {current}/{total}", event.message),
        _ => event.message.clone(),
    }
}
