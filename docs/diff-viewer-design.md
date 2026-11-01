# P4k and DataCore Diff Viewer Design

## Goal

StarBreaker should provide a user-friendly way to compare Star Citizen
`Data.p4k` versions across both the CLI and the Tauri app.

The feature answers two questions:

- which files inside the P4k were added, removed, modified, or changed only in
  archive metadata
- which DataCore records were added, removed, modified, or changed only in
  display metadata

The CLI and GUI must use the same core diff engine and report schema so their
results stay in parity.

## Non-Goals

Version 1 does not compute arbitrary file content diffs. It does not
decompress and compare every changed binary, texture, geometry, XML, Wwise
asset, or other file payload.

Version 1 also does not provide multi-version timeline diffing, first-class
rename detection, or a custom query language.

Future versions may use the changed-file list to open selected files or
DataCore records side by side when the original sources are available.

## Architecture

Add a shared Rust crate, tentatively `crates/starbreaker-diff`.

The crate owns:

- inventory report schema
- diff result schema
- P4k archive inventory generation
- DataCore record inventory generation
- canonical DataCore hashing
- report JSON read/write
- report comparison
- shared filter predicates
- progress and cancellation hooks

The crate does not own:

- CLI table rendering
- Tauri DTOs beyond serializable domain structs
- React state, layout, icons, colors, or labels
- persistent GUI cache policy

Thin adapters call the crate:

- `cli/src/diff.rs` exposes commands and terminal formatting
- `app/src-tauri/src/diff_commands.rs` exposes Tauri commands and progress
  events
- `app/src/views/diff-view.tsx` renders the interactive diff view

## Inventory Reports

Inventory reports are reusable, portable JSON snapshots named:

```text
*.starbreaker-inventory.json
```

Reports include all archive entries and, unless skipped, all DataCore records.
They are not just changed-item outputs.

Reports are plain JSON in version 1. If real reports become too large,
`*.starbreaker-inventory.json.gz` can be added later without changing the core
schema model.

### Report Metadata

Reports include technical provenance and schema metadata:

- `schema_version`
- `mode`
- `generated_by`
- `generated_at`
- `hash_algorithms`
- `source_file`
- optional `build_manifest`
- optional display `label`
- `inventory_hash`

`label`, source path, source modified time, and generation time are display or
provenance metadata. They are not part of `inventory_hash`.

`inventory_hash` is a stable technical hash over normalized inventory content
and hash algorithm identifiers. It should not change when a user renames a
source label.

### Source Labels

When the source is a `Data.p4k`, StarBreaker should look for
`build_manifest.id` beside it. If found and valid, parse:

- `Data.Branch`
- first `x.y.z` version pattern from the branch string, when present
- `Data.RequestedP4ChangeNum`
- channel hint from the parent folder name, such as `LIVE`, `PTU`, or `HOTFIX`

The default label order is:

1. user-provided label
2. parsed manifest label, such as `4.7.0 LIVE.123456`
3. parent folder plus source modified date
4. `Data.p4k`

Missing `build_manifest.id` is normal and must not fail inventory generation.
Invalid manifest JSON should produce a warning and fall back to the next label
strategy.

### Inventory Modes

Default mode is full inventory:

- archive entries
- DataCore records

An explicit advanced mode may skip DataCore:

- CLI flag: `--skip-datacore`
- report mode: `p4k_only`
- GUI wording: `Skip DataCore record inventory`

If DataCore is not skipped, missing or unparseable DataCore is an inventory
failure. Star Citizen `Data.p4k` is expected to contain readable DataCore data;
failure likely means the game format changed or StarBreaker cannot read the
archive correctly.

## P4k Archive Inventory

Archive inventory uses central-directory metadata and does not read or
decompress every file.

For each file entry, store:

- original path
- normalized path key
- CRC32
- compressed size
- uncompressed size
- compression method
- encrypted flag
- last modified value, if available

The existing P4k layer already exposes `name`, `compressed_size`,
`uncompressed_size`, `compression_method`, `is_encrypted`, `crc32`, and
`last_modified`.

### Archive Identity

Archive content identity is based on:

- `crc32`
- `uncompressed_size`

If a stronger archive-provided content hash is exposed later, it can be added
with a new versioned archive identity algorithm.

Full-file hashing of `Data.p4k` is intentionally avoided. The report's
technical identity is the inventory hash, not a cryptographic hash of the
massive source archive.

### Archive Path Matching

Archive paths match by normalized identity:

- normalize separators
- match case-insensitively
- preserve original spelling for display

If only path spelling, casing, or separator style changes, do not show the file
as added and removed. Match it as the same entry and record a metadata/path
reason.

### Archive Status

Archive statuses:

- `added`: normalized path exists only in new
- `removed`: normalized path exists only in old
- `modified`: same normalized path exists in both, but content identity changed
- `metadata_changed`: content identity is unchanged, but metadata changed
- `unchanged`: no relevant change

Archive change reasons may include:

- `crc32_changed`
- `uncompressed_size_changed`
- `compressed_size_changed`
- `compression_method_changed`
- `encrypted_changed`
- `last_modified_changed`
- `path_case_changed`
- `path_separator_changed`

`last_modified` alone must not make a file content-modified.

Version 1 does not classify renames. A removed path and added path remain
separate changes, even if their content identity matches. Later versions may
show possible move/copy hints.

## DataCore Inventory

DataCore inventory stores one entry per main DataCore record.

For each record, store:

- GUID identity key
- record type / struct name
- record name
- record file path
- canonical content hash

The GUID is the comparison key. It is not included in the content hash.

Record name and file path are display metadata. They are not included in the
content hash.

The record type is part of the canonical content hash because a type change
changes how the payload should be interpreted.

### Canonical DataCore Hashing

DataCore hashes must represent decoded semantic content, not raw byte spans in
the DCB.

The canonical hash includes:

- type identity
- property names and types
- primitive values
- arrays in stored order
- strings resolved to text
- enum values resolved to names
- GUID values
- references represented by referenced GUIDs

The canonical hash excludes:

- record GUID
- record display name
- record file path
- source path
- report label
- generation time

References are not recursively expanded. If record A references record B and B
changes, only B is modified unless A changes its own referenced GUID or other
stored content.

Use a versioned algorithm name, for example
`starbreaker-dcb-canonical-v1`, and a strong hash such as BLAKE3 or SHA-256.

### DataCore Status

DataCore statuses:

- `added`: GUID exists only in new
- `removed`: GUID exists only in old
- `modified`: GUID exists in both and canonical content hash changed
- `metadata_changed`: content hash is unchanged, but display name or record
  path changed
- `unchanged`: no relevant change

If record type changes, classify it as `modified` with reason `type_changed`.

DataCore change reasons may include:

- `content_hash_changed`
- `type_changed`
- `name_changed`
- `path_changed`

## Diff Results

Saved diff results are optional and named:

```text
*.starbreaker-diff.json
```

The GUI normally computes diffs in memory from two inventories. Users can
export a saved diff result when they need to share or script against it.

Saved diff results include:

- schema version
- old/new source summaries
- old/new inventory hashes
- summary counts
- changed items by default
- filter/output metadata, if generated with filters

Unchanged items are omitted from saved diff files by default. A CLI flag such
as `--include-unchanged` may include them.

## CLI Design

Add a top-level `diff` command with separate subcommands:

```text
starbreaker diff inventory <SOURCE> -o <REPORT> [--skip-datacore] [--label <LABEL>]
starbreaker diff compare <OLD> <NEW> [-o <DIFF>] [filters...]
```

`<SOURCE>` for inventory is normally `Data.p4k`.

`<OLD>` and `<NEW>` for compare may each be:

- a `Data.p4k`
- a `*.starbreaker-inventory.json`

If compare receives a P4k source, it generates a temporary inventory in memory.

Recommended compare filters:

- `--tier all|p4k|datacore`
- `--status added,removed,modified,metadata,unchanged`
- `--search <text>`
- `--extension <ext>`
- `--record-type <type>`
- `--path-prefix <prefix>`
- `--include-unchanged`
- `--format table|json`

Progress should be written to stderr. Machine-readable JSON should go to stdout
or the file specified by `-o`.

## GUI Design

Add a Diff view with two source slots:

- Old
- New

Each slot accepts either:

- `Data.p4k`
- `*.starbreaker-inventory.json`

If a slot receives a P4k, the app generates an inventory first. The generated
inventory is cached in memory for the current session. It is persisted only
when the user explicitly chooses to save it.

Inventory generation runs as a cancellable background task with progress
events:

- opening P4k
- reading archive index
- reading DataCore
- hashing DataCore records
- writing report, if requested

The main result view is one unified table rather than separate archive and
DataCore screens.

Recommended layout:

- source summary bar for old/new
- summary counts for added, removed, modified, metadata-only, unchanged
- facet rail for tier, status, extension, record type, and path prefix
- virtualized result table
- detail panel for selected item metadata and change reasons

The table should support simple text search plus explicit facets. Do not build
a custom query language in version 1.

Text search should match:

- archive path
- archive filename
- archive extension
- DataCore record name
- DataCore record type
- DataCore record path
- DataCore GUID

Large result sets require virtualization from the first implementation.

## Shared Filters

Filter semantics belong in `starbreaker-diff` so CLI and GUI stay consistent.
The GUI owns view state and rendering.

Shared filters should cover:

- text search
- tier
- status
- extension
- record type
- path prefix
- include unchanged

## Sorting and Stability

Reports preserve original display values but sort entries canonically.

Recommended report order:

- archive entries by normalized path
- DataCore records by GUID

The GUI may sort differently for display.

Stable sorting keeps report JSON deterministic, makes inventory hashes
repeatable, and makes tests easier to reason about.

## Testing Strategy

Automated tests should not require real Star Citizen P4k files.

Test `starbreaker-diff` with small and synthetic fixtures:

- report JSON roundtrip
- inventory hash stability
- label and timestamp excluded from inventory hash
- archive added/removed/modified/metadata-only statuses
- DataCore added/removed/modified/metadata-only statuses
- path normalization and case handling
- skip-DataCore mode compatibility
- filter semantics
- saved diff omission of unchanged items by default

If practical, add tiny archive fixture tests for P4k central-directory metadata.
If practical, use existing DataCore builder/test helpers to generate tiny DCB
fixtures for canonical hashing tests.

Real P4k validation should be manual or env-gated, for example:

```text
SC_DATA_P4K=... cargo test -p starbreaker-diff -- --ignored
```

## Future Work

Possible future extensions:

- selected file side-by-side viewers
- selected DataCore record field diffs
- possible rename/move/copy hints
- gzip-compressed reports
- persistent GUI inventory cache with invalidation
- multi-version timeline comparison
- advanced query syntax
- MCP tools for inventory and report inspection
