# Compatibility matrix

The contract standard is **one application backend, both TUI and iced pass**.
Compilation plus the core interaction path is the bar; feature parity is not.

## Current status

| Application | Core actor | TUI | iced | Core path exercised |
| --- | --- | --- | --- | --- |
| `apps/player` | ✅ | ✅ | ✅ | TUI launches and quits with `Ctrl+Q`; iced builds against the same `Player` view model. |
| `apps/editor` | ✅ | ✅ | ✅ | TUI draws a `TestBackend` snapshot, opens a file, inserts text, saves, and quits; iced launches with a live snapshot and close-request interception. |
| `apps/yazi-desktop` | ✅ | ✅ | ✅ | TUI draws the shared `ViewModel`, navigates, filters, and previews; iced file-manager path builds and opens the layered config. |
| `boxpigma` (external `boxpigma` repo) | ✅ | ✅ | ✅ | Pins Ferriso `55d1745` (0.3.1); workspace tests pass, TUI starts/renders/quits, and iced opens/closes a live window. See [`versioning.md`](versioning.md). |

`—` means the frontend has not been attached. It is not a claim that the
application cannot be attached.

GPUI and Floem remain boundary probes. They are deliberately outside the P0
matrix until the TUI + iced path stays clean across every application.
`apps/twozero48` is a GUI-only downstream probe and is also outside the
two-frontend matrix.

## Release smoke: 0.3.1

Run on the 2026-10-25 desktop session after rebuilding the binaries from the
release source. TUI checks used a 120×40 pty and quit with `Ctrl+Q` after the
first frame. Iced checks found the live window with `xdotool` and sent the
window-manager close chord (`Alt+F4`); both applications exited cleanly.

| Path | Binary | Result |
| --- | --- | --- |
| Player TUI | `ferriso-player-tui` | Rendered a frame containing `Ferriso`; exit 0. |
| Player iced | `ferriso-player-gui` | Window opened; close request exited 0. |
| Editor TUI | `ferriso-editor-tui README.md` | Rendered `Ferriso`, `README`, and the `Ctrl+Q` status line; exit 0. |
| Editor iced | `ferriso-editor README.md` | Window opened; close request exited 0. |

The headless release check was `./scripts/compat-check.sh`; the current
post-change suite is `cargo test --workspace` with 258 passing tests across 49
suites.

## What "pass" means

1. `cargo test --workspace` passes.
2. The application constructs an actor-backed `App`; the frontend never calls
   `App::update` or owns domain state.
3. TUI: the binary enters the alternate screen, renders a snapshot, accepts a
   semantic edit or navigation key, and exits without panicking.
4. iced: the binary launches, renders the boot snapshot, and exits cleanly when
   the window closes. Headless CI compiles this path; the live-window check is
   run on a desktop runner.
5. Configuration is loaded outside the core and passed into the application
   constructor. The core keeps its Tokio/diagnostics dependency boundary.

## Reproduce

```bash
./scripts/compat-check.sh
```

Individual paths:

```bash
# Contract and adapters
cargo test -p ferriso-core -p ferriso-config -p ferriso-tui -p ferriso-gui -p ferriso-workspace

# Native service feature matrix
cargo test -p ferriso-platform
cargo test -p ferriso-platform --no-default-features

# Player, both frontends
cargo test -p ferriso-player
cargo run -p ferriso-player --bin ferriso-player-tui
cargo run -p ferriso-player --bin ferriso-player-gui

# yazi-desktop, both frontends
cargo test -p ferriso-yazi-desktop
cargo run -p ferriso-yazi-desktop --bin yazi-desktop-tui
cargo run -p ferriso-yazi-desktop

# Editor, both frontends
cargo test -p ferriso-editor-app
cargo run -p ferriso-editor-app --bin ferriso-editor-tui -- README.md
cargo run -p ferriso-editor-app --bin ferriso-editor -- README.md
```

`scripts/compat-check.sh` runs the headless part of this list plus formatting,
the dependency-boundary checks in `scripts/architecture-check.sh`, clippy with
warnings denied, and the full workspace test suite. The GitHub workflow
`.github/workflows/compatibility.yml` runs the same script.

The separate `windows-framework` job runs the core/configuration/diagnostics,
editor, platform, workspace, keymap, TUI, and test-harness suites on
`windows-latest`, plus the feature-free platform build. A Windows 11 VM run of
the same framework surface passed 115 tests across twelve executables, including
the shared TUI/iced conformance test and the async platform dialog helper; the
editor app test binaries passed 33 Windows app/TUI tests, including the async
open/save/save-all/close-save dialog flows, synchronous save snapshot refreshes,
the pending-dialog mutation guard, cursor, and welcome-overlay regressions.
The feature-free platform binary passed
its single test. Live window checks remain desktop-only.

`boxpigma` is not a workspace member because it is a downstream consumer. It
pins Ferriso `55d1745` (0.3.1). To check it against its pinned revision:

```bash
cd ../boxpigma
cargo test --workspace
cargo run --bin boxpigma
cargo run --bin boxpigma-gui
```

Its GUI binary is `boxpigma-gui`; the terminal binary is `boxpigma`. The smoke
check started the TUI, observed a rendered frame, and quit with `q`; the iced
window opened and exited cleanly on `Alt+F4`.

## Adapter translation budget

Translation code means key/event mapping, layout, rendering, relay, and
runtime/window startup. Domain state, search state, tab state, and save policy
must stay in the application or kernel.

The editor is the largest adapter surface: a real text editor needs a canvas
and a terminal renderer. Its TUI adapter is kept to input translation and cell
drawing; tabs, search matches, dirty state, and save prompts live in
`EditorModel`. If a future change pushes editor behavior into `app.rs` or
`tui.rs`, fix the kernel/contract boundary before adding another special case.
