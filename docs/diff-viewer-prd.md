# Diff Viewer PRD

## Problem Statement

Star Citizen updates change both raw files inside `Data.p4k` and structured
records inside DataCore. StarBreaker users need a fast, understandable way to
compare two game versions without extracting or line-diffing the entire
archive.

Today a user can browse P4k files and DataCore records, but they cannot produce
a reusable inventory snapshot or answer "what changed between these two
versions?" from either the CLI or the GUI.

## Solution

Build a shared diff engine that inventories P4k archive entries and DataCore
records, writes portable inventory reports, compares two sources or reports,
and exposes the same results through the CLI and Tauri GUI.

The first version focuses on change discovery:

- P4k file added, removed, modified, metadata-only, unchanged
- DataCore record added, removed, modified, metadata-only, unchanged
- searchable and filterable results
- reusable `*.starbreaker-inventory.json` reports
- optional `*.starbreaker-diff.json` outputs

Full file content diffing and side-by-side viewers are future work.

## User Stories

1. As a StarBreaker CLI user, I want to generate an inventory report from a
   `Data.p4k`, so that I can reuse it for later comparisons.
2. As a StarBreaker GUI user, I want to select a `Data.p4k` directly, so that I
   do not need to understand the report format before comparing versions.
3. As a power user, I want to compare two saved inventory reports, so that I do
   not need both massive P4k files available.
4. As a user with one local P4k and one saved report, I want to compare those
   sources directly, so that I can work with whichever artifacts I have.
5. As a modding/research user, I want to see changed archive files by path and
   extension, so that I can inspect changed asset categories quickly.
6. As a DataCore researcher, I want to see changed records by type, name, path,
   and GUID, so that I can identify changed ships, items, components, and
   records without reading the whole DCB.
7. As a GUI user, I want inventory generation to run in the background with
   progress and cancellation, so that the app remains responsive.
8. As a CLI user, I want progress on stderr and machine-readable output on
   stdout or a file, so that scripts can consume the command reliably.
9. As a user comparing two patches, I want content changes separated from
   metadata-only changes, so that packaging noise does not hide meaningful
   changes.
10. As a user, I want simple text search and explicit filters, so that I can
    narrow results without learning a query language.
11. As a GUI user, I want one unified results table for files and records, so
    that I can search across the full patch surface in one place.
12. As a CLI user, I want filter flags that match the GUI semantics, so that
    CLI and GUI results agree.
13. As a user, I want missing `build_manifest.id` to be tolerated, so that I
    can still inventory P4ks copied away from their install folder.
14. As a user, I want StarBreaker to use `build_manifest.id` when present, so
    that reports get useful default labels.
15. As a user, I want to rename the source label, so that reports can use my
    own version naming.
16. As a user, I want source labels not to affect technical inventory identity,
    so that renaming a report does not change what it represents.
17. As a user, I want reports to be plain JSON, so that they are easy to inspect
    and share.
18. As a developer, I want the diff engine in a shared crate, so that CLI and
    GUI cannot drift apart.
19. As a developer, I want stable inventory hashes, so that snapshots can be
    compared and tested deterministically.
20. As a developer, I want synthetic automated tests, so that normal tests do
    not require proprietary large P4k files.
21. As an advanced user, I want an explicit option to skip DataCore inventory,
    so that I can still inspect raw archive changes in debug or degraded
    workflows.
22. As a normal user, I want full inventory to fail if DataCore cannot be read,
    so that serious parser or format failures are not hidden.
23. As a future user, I want changed items to retain enough identity to open
    side-by-side views later, so that v1 reports can support future detail
    workflows.

## Implementation Decisions

- Build a new shared Rust crate for diff data structures, inventory generation,
  comparison, canonical hashing, report IO, and shared filter semantics.
- Keep CLI and Tauri code as adapters over the shared crate.
- Use plain JSON inventory reports named `*.starbreaker-inventory.json`.
- Use optional plain JSON diff result files named `*.starbreaker-diff.json`.
- Treat inventory reports as reusable snapshots that include all archive
  entries and all DataCore records unless DataCore was explicitly skipped.
- Treat saved diff results as optional outputs that omit unchanged rows by
  default while preserving summary counts.
- Accept both raw P4k files and saved inventory reports as compare inputs.
- Generate temporary in-memory inventories when a compare input is a P4k.
- Cache GUI-generated inventories in memory for the current session only.
- Persist inventory reports only when the user explicitly saves them.
- Use cancellable inventory generation with progress callbacks/events.
- Use P4k central-directory metadata for archive inventory. Do not decompress
  every file.
- Match archive entries by normalized case-insensitive path while preserving
  original display paths.
- Classify archive content changes by `crc32` plus `uncompressed_size`.
- Classify archive metadata-only changes separately from content changes.
- Do not classify renames in version 1.
- Hash DataCore records by canonical decoded semantic content, not raw DCB byte
  spans.
- Match DataCore records by GUID.
- Exclude record GUID, record name, and record path from DataCore content hash.
- Include record type in DataCore content hash.
- Represent DataCore references by referenced GUID and do not recursively hash
  referenced records.
- Treat DataCore record name/path changes as metadata-only when content hash is
  unchanged.
- Treat DataCore record type changes as modified.
- Version all hash algorithms in the report.
- Avoid full-file hashing of `Data.p4k`; use source file metadata and an
  inventory-level hash instead.
- Parse `build_manifest.id` beside `Data.p4k` when present, but tolerate
  missing or invalid manifests.
- Provide a clear `--skip-datacore` CLI flag and equivalent advanced GUI label
  if the GUI exposes it.
- Keep shared filters in the core crate, but keep UI state and presentation in
  the GUI.
- Use virtualization in the GUI result table from the first implementation.

## CLI Requirements

Initial commands:

```text
starbreaker diff inventory <SOURCE> -o <REPORT> [--skip-datacore] [--label <LABEL>]
starbreaker diff compare <OLD> <NEW> [-o <DIFF>] [filters...]
```

Compare inputs may be P4k files or inventory reports.

Recommended filters:

- `--tier all|p4k|datacore`
- `--status added,removed,modified,metadata,unchanged`
- `--search <text>`
- `--extension <ext>`
- `--record-type <type>`
- `--path-prefix <prefix>`
- `--include-unchanged`
- `--format table|json`

## GUI Requirements

The Diff view should contain:

- two source slots, Old and New
- support for selecting P4k files or inventory reports in either slot
- background inventory generation for P4k sources
- progress and cancellation
- save inventory action for generated inventories
- compare action when both sources are ready
- summary counts
- unified virtualized result table
- facet rail for tier, status, extension, record type, and path prefix
- simple text search
- detail panel for selected item identity, old/new metadata, and change reasons
- optional export diff action

## Trackable Goals

### Milestone 1: Shared Schema and Fixtures

- Create the shared diff crate.
- Define inventory report structs.
- Define diff result structs.
- Define status, tier, reason, and filter enums/structs.
- Implement JSON read/write.
- Implement canonical report sorting.
- Implement inventory hash calculation that excludes display/provenance fields.
- Add synthetic report fixtures.
- Add schema roundtrip and inventory hash tests.

Acceptance criteria:

- A test can write and read an inventory report without data loss.
- Changing a label or generated timestamp does not change `inventory_hash`.
- Changing a technical archive or DataCore entry does change `inventory_hash`.

### Milestone 2: Report Comparison Engine

- Implement archive comparison.
- Implement DataCore comparison.
- Implement summary counts.
- Implement changed-item result generation.
- Implement optional unchanged inclusion.
- Implement shared filters.
- Add tests for added, removed, modified, metadata-only, unchanged, path
  normalization, and filter semantics.

Acceptance criteria:

- Synthetic old/new reports produce expected statuses and reasons.
- Archive metadata-only changes are not reported as content modifications.
- DataCore name/path changes are metadata-only when content hash is unchanged.
- CLI and GUI adapters can use the same filter object.

### Milestone 3: Inventory Generation

- Generate P4k archive inventory from central-directory metadata.
- Parse source file metadata.
- Parse optional `build_manifest.id`.
- Generate DataCore record inventory.
- Implement canonical DataCore record hashing.
- Implement `--skip-datacore` behavior in the core inventory options.
- Add progress and cancellation plumbing.
- Add small fixture tests where practical.

Acceptance criteria:

- Full inventory fails if DataCore is missing or unparseable and DataCore was
  not skipped.
- Missing `build_manifest.id` does not fail inventory generation.
- `--skip-datacore` produces a `p4k_only` report with DataCore marked skipped.
- Inventory generation reports progress and respects cancellation.

### Milestone 4: CLI Adapter

- Add `starbreaker diff inventory`.
- Add `starbreaker diff compare`.
- Support P4k and report inputs for compare.
- Support table and JSON output.
- Support writing inventory and diff files.
- Support shared filter flags.
- Print progress to stderr.
- Add CLI smoke tests or command-level tests where practical.

Acceptance criteria:

- A user can generate an inventory report from a P4k.
- A user can compare two reports.
- A user can compare a P4k and a report.
- Filtered CLI results match core filter tests.

### Milestone 5: Tauri Commands

- Add Tauri commands for selecting/loading inventory reports.
- Add Tauri commands for generating inventories from P4k sources.
- Add Tauri commands for comparing inventories.
- Emit progress events.
- Support cancellation.
- Keep generated inventories in session memory.

Acceptance criteria:

- The app can load two saved reports and compare them.
- The app can generate an inventory from a P4k without blocking the UI thread.
- Cancelling an inventory job stops work and returns a clear state.

### Milestone 6: GUI Diff View

- Add the Diff navigation entry/view.
- Build Old/New source slots.
- Build source summary cards with labels and manifest-derived defaults.
- Build progress/cancel UI.
- Build summary counts.
- Build unified virtualized result table.
- Build facets and text search.
- Build selected item detail panel.
- Add save inventory and export diff actions.

Acceptance criteria:

- A user can compare two reports from the GUI.
- A user can compare two P4ks from the GUI.
- The result table remains responsive with large synthetic result sets.
- Search and facets match CLI/core behavior.

### Milestone 7: Documentation and Manual Validation

- Document CLI usage.
- Document GUI workflow.
- Document report modes and limitations.
- Document manual real-P4k validation.
- Add examples for LIVE vs PTU comparison.

Acceptance criteria:

- A user can follow docs to generate two inventories and compare them.
- The docs explain that v1 does not perform full file content diffs.
- The docs explain `--skip-datacore` and its consequences.

## Testing Decisions

Automated tests should focus on externally observable behavior:

- report roundtrip
- inventory hash stability
- comparison statuses
- change reasons
- filter semantics
- skip-DataCore behavior
- manifest parsing fallbacks
- cancellation behavior where practical

Tests should avoid asserting implementation details such as internal helper
function boundaries or temporary allocation strategy.

Real Star Citizen P4k files should not be required for normal tests. Use
synthetic reports and tiny generated fixtures. Real P4k validation should be
manual or ignored/env-gated.

## Out of Scope

- full content diffing for arbitrary P4k files
- automatic rename classification
- multi-version timeline comparison
- persistent GUI inventory cache
- compressed report format
- custom query language
- publishing reports to a remote service
- changing existing P4k browser or DataCore browser behavior except where
  shared helpers are reused

## Open Questions

- Should DataCore canonical hashing reuse compact JSON export initially, or
  should it start with a dedicated hashing sink?
- Which hash crate should be used for canonical content and inventory hashes:
  BLAKE3 or SHA-256?
- Can the existing P4k reader open tiny standard ZIP fixtures directly, or do
  tests need purpose-built fixture helpers?
- Which React virtualization library best fits the current app dependency
  policy?
- Should `--skip-datacore` be exposed in the GUI v1 or only available through
  the CLI/debug path?

## Further Notes

The design intentionally makes reports portable and reusable. Source paths,
manifest data, and labels help humans understand reports, but comparison
correctness comes from the inventory entries and their versioned hash
algorithms.

The most important engineering constraint is keeping comparison behavior in
the shared Rust crate. CLI and GUI parity depends on having one implementation
of inventory generation, comparison, status classification, and filtering.
