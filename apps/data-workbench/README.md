# Ferriso Data Workbench

A dogfood application for Ferriso's shared table model and both native
frontends.

The fixture uses a Modrinth-like project catalog: `id`, `slug`, `title`,
`summary`, `author`, `kind`, `loaders`, `game_versions`, `downloads`,
`followers`, `updated`, `license`, `status`, and `size_bytes`. It deliberately
includes Unicode, empty summaries, missing licenses, multiple loaders, and
outlier download counts.

- 100,000 deterministic rows
- one actor-owned filter and selection
- `ferriso-workspace::TableState` visible-range rendering
- TUI and iced views over the same `ViewModel`
- no database, HTTP, table, or UI framework dependencies

```bash
cargo run -p ferriso-data-workbench --bin ferriso-data-workbench-tui
cargo run -p ferriso-data-workbench --bin ferriso-data-workbench-gui
cargo test -p ferriso-data-workbench
```

TUI: type to filter, `↑/↓`, `PgUp/PgDn`, `Home/End`, `Esc` clears, `Ctrl+Q`
quits. GUI uses the same keyboard navigation and a filter field.

The first version intentionally uses an in-memory fixture. A real SQLite or
HTTP source should be added app-locally rather than in `ferriso-core`.
