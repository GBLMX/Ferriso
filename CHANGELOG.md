# Changelog

## Unreleased

### Added

- `apps/ferriso-cli`: a Clawless-based `ferriso` command with `version`, `apps`,
  `crates`, and `doctor` subcommands. The catalog and toolchain commands emit
  structured artifacts, so the global `--json` flag produces machine-readable
  output instead of formatted text.

- `apps/data-workbench`: a 100k-row Modrinth-like TUI/iced table dogfood app
  built on `ferriso-workspace::TableState`, with responsive viewports and
  shared filtering/selection across both frontends.

- `apps/terminal`: an app-local embedded terminal prototype using `rio-vt` for
  PTY/VT/grid state and an iced cell renderer, with resize and IME support.

- `ferriso-platform`: `pick_file_async`, `pick_folder_async`, and
  `save_file_async` run any `Arc<dyn FileDialog>` on Tokio's blocking pool and
  return `DialogError` when the worker fails.
- `ferriso-core` and adapters: `CoreContext`, `Sidecar`/`Sidecars`, and
  `run_with_app_context`/`run_with_app_services` expose the actor endpoints
  during startup and stop IPC, MPRIS, or other services before actor shutdown.
  `CoreHandle::request_exit`/`CoreContext::request_exit` name the sidecar-facing
  clean exit; `shutdown` remains an alias. See `docs/sidecar.md`.
- `ferriso-gui::motion`: renderer-local easing curves and a clamped
  `Animation` timeline for short transitions.
- `ferriso-gui::art::Pulse`: animated concentric pulse rings for focus,
  activity, and live beacons, demonstrated in the design gallery.
- `ferriso-tui::art`: terminal-native gradients, animated aurora background,
  bounded pulse envelope, and moving text shimmer, all driven by caller-supplied
  time.
- `ferriso-editor`: re-export `DEFAULT_OPEN_LIMIT` so the public `Document::open`
  bound is linkable and discoverable.

### Documented

- Documentation builds with warnings denied; the compatibility script and CI
  use `--locked` resolution and check rustdoc before tests.
- CI now checks the declared Rust 1.88 MSRV with a locked workspace check.
- Clarified that `boxpigma` is a same-author downstream fork of pigma and a
  controlled compatibility probe, not independent adoption.

### Removed

- `ferriso-core`: removed the deprecated `App::tick_interval` and `App::tick`
  hooks and the actor-owned compatibility timer. Timed work now uses
  `Subscription::interval` and an application-defined tick message.

### Fixed

- `ferriso-editor`: `Document::save_as` now writes a same-directory temporary
  file and renames it into place, so a failed save cannot truncate the previous
  file. Existing ordinary mode bits are preserved; new Unix files start as
  `0600`. `Document::open` is bounded at 64 MiB and `open_with_limit` exposes an
  explicit bound.
- `ferriso-cli`: the `apps` catalog now includes `data-workbench` and the CLI
  itself; a catalog test guards against future workspace apps being omitted.
- `apps/editor`: Open, Save, Save As, Save All, and close-save dialogs run as
  actor commands instead of blocking `update`; dialog results are applied once,
  duplicate dialogs are suppressed, and Save All resumes one tab at a time.
  `EditorModel::with_dialog` injects a backend for headless tests.
- `apps/editor`: opening a real file or a new buffer now shows the caret at
  column 0; the canvas no longer clips a caret drawn exactly on the text-area
  edge, and the welcome screen is dismissed by New/Open/typing. The TUI cursor
  placement has a regression test.
- `ferriso-diagnostics`: a panicking logger is caught and disabled instead of
  propagating into the actor.
- `ferriso-tui`: mouse capture is disabled by a guard when the event loop
  unwinds; ratatui already restores raw mode and the alternate screen.
- `ferriso-floem`: the snapshot relay no longer blocks forever on a full
  bounded channel when `Relay::stop` is called.
- `ferriso-platform`: file watcher queues are bounded, and system opener child
  processes are reaped.
- `ferriso-gui`: palette and combobox navigation now wraps with `i128` arithmetic
  so extreme caller-supplied deltas cannot overflow; `Combobox::set_options`
  preserves a still-valid highlight, and art primitives treat non-finite
  samples, progress, gaps, and radii as bounded fallbacks.
- `ferriso-editor` and `ferriso-diagnostics`: repaired broken rustdoc
  intra-doc links.

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
