# Changelog

## Unreleased

### Added

- `apps/ferriso-cli`: a Clawless-based `ferriso` command with `version`, `apps`,
  `crates`, and `doctor` subcommands. The catalog and toolchain commands emit
  structured artifacts, so the global `--json` flag produces machine-readable
  output instead of formatted text.

## 0.3.1 - 2026-10-25

### Fixed

- `Command::batch` drops earlier entries that share a key before spawning. An
  immediately-ready command can no longer deliver a message in the gap before
  a later command with the same key replaces it.

### Documented

- Recorded live TUI and iced smoke checks for the player and editor against
  this release in `docs/compatibility.md`.

## 0.3.0 - 2026-10-25

### Added

- `Command::perform_keyed`: latest-wins commands. Dispatching a command with an
  existing key aborts the previous future; the actor tracks keyed tasks by key
  and only untracks the result that belongs to the current task.
- `ferriso-gui::interaction::Combobox`: searchable dropdown state with fuzzy
  label/detail filtering, disabled-option-aware keyboard navigation, paging,
  explicit dismissal, and an inline view.

## 0.2.0 - 2026-10-23

### Changed

- A command or subscription task panic is now fatal: the runtime records the
  framework error, stops the actor, and runs the shutdown path. `App::debug`
  panics remain diagnostic-only.
- `App::tick_interval` and `App::tick` are deprecated compatibility hooks.
  New applications use `Subscription::interval` with their own `Tick` message.
  The player is migrated; the hooks remain until downstream consumers move.
- Dropping a frontend handle and natural subscription end are documented as
  distinct from actor failure. A stream that ends emits a debug event but keeps
  its key until the application stops returning it.

### Added

- `ferriso-keymap`: dependency-free key chords, binding parsing, and
  frontend-neutral shortcut maps, with iced and Crossterm converters.
- `Subscription::interval` for application-owned clocks.
- `Subscription::stream_with_end` for observing stream completion.
- `App::shutdown_async` for flushing asynchronous resources before synchronous
  shutdown.
- `Snapshots::revision`, `Snapshots::current_with_revision`, and
  `Snapshots::changed_with_revision`.
- Adapter conformance test driving one test application through the ratatui and
  iced renderer surfaces and both snapshot readers.
- Versioning policy in `docs/versioning.md`.
- `apps/twozero48`: an animated GUI-only 2048 probe built on the core and iced
  adapter, using the upstream `twozero48` board rules. Tiles slide from their
  source cells, merges bounce, the score pulses, and spawned tiles pop in.
- `ferriso-gui::art::{Bars, Gauge}`: compact live bar strips and bounded
  circular progress gauges for data, media, and launcher surfaces.
- `ferriso-gui::interaction`: fuzzy ranked command search, plus Home, End,
  Page Up, and Page Down navigation in the palette.
- `ferriso-platform`: `clipboard`, `dialogs`, `notifications`, and `watch`
  are independent features. Workspace consumers enable only the backends they
  use; the standard-library opener stays unconditional.
- `ferriso-workspace`: reusable `Tabs<T>`, `SplitPane<K>`, and `TableState`
  models for tab lifecycle, two-pane focus/ratio, and virtualized fixed-row
  tables. The crate has no toolkit dependency, so TUI and GUI apps share the
  same models without linking iced. The models moved out of
  `ferriso-gui::workspace`; import `ferriso_workspace` instead.
- The design gallery now combines the aurora, orbit, bars, gauge, skins,
  palette, and toasts; the workspace gallery demonstrates tabs, split panes,
  and 10,000-row virtualization.

### Documented

- `docs/art-interaction-study.md` records the visual and interaction decisions
  extracted from the GUI research list.
- Frontend input is intentionally unbounded; frontends own coalescing for
  high-frequency input, while snapshots coalesce automatically.
- In-flight commands and subscriptions are application-owned; the runtime does
  not silently cap their fan-out.
- `docs/ecosystem-study.md` maps the external GUI ecosystem onto Ferriso's
  boundary and records the adoption/defer/reject decisions.
- `scripts/architecture-check.sh` guards the dependency direction: the core
  stays renderer-free, TUI and iced adapters stay independent, and
  `ferriso-workspace` plus the feature-free platform crate stay std-only.
