# Ecosystem study

This note maps the external projects in the GUI research list onto Ferriso's
actor + adapter architecture. It is deliberately opinionated: the goal is not
to grow a second GPUI, but to identify the small number of framework facilities
that keep real applications possible without collapsing the core boundary.

## Decision summary

Adopt:

- A frontend-neutral keyboard shortcut layer. Keyboard-first editors and tools
  repeatedly converge on the same primitive, and Ferriso currently duplicates
  raw key matching in every adapter.
- Adapter-local state for focus, text drafts, scroll offsets, pending
  animation, and selection. Snapshots stay immutable and domain-owned.
- Latest-only snapshots for domain state. Fine-grained signals belong inside an
  adapter, not between the actor and a frontend.
- Frame-driven animation owned by the renderer. The 2048 probe proves this can
  be done without waking the actor.
- Headless conformance tests as the contract bar. Live-window rendering remains
  a manual/desktop-runner check.

Defer:

- Multi-window orchestration. The core already supports several frontends; a
  single adapter opening several native windows needs its own design.
- Accessibility semantics. Important, but a data-only `View` cannot promise a
  useful accessibility tree until each adapter maps it explicitly.
- A generic virtual-list widget. Large-data apps need viewport data and paging,
  which are application policy, not core state.
- A component marketplace or plugin ABI. `design` and `interaction` should stay
  small, copyable templates until real reuse justifies more.

Reject:

- Moving widget trees, layout, GPU state, or focus into `ferriso-core`.
- Embedding a WebView or Qt/QML as the framework's required UI path.
- Copying GPUI's entity/context API wholesale. Its model is sound for GPUI;
  Ferriso's contract is different.
- A second domain-state store in the frontend for convenience.

## What each cluster teaches

### GPUI applications

Projects: Fulgur, Arbor, Codux, Farcaster, GitComet, Rulogman, Baudrun, GetCat,
arc, DBFlux, DBDelve, FluxDB, based, dbui. Zed is the reference framework
project and remains out of scope as a comparison target.

Shared patterns:

- GPU-native rendering with a retained scene, not a widget tree rebuilt from a
  domain snapshot.
- Entity/context separation: long-lived entities hold state, context routes
  effects and focus, and tasks are tied to an entity lifetime.
- Actions and focus make keyboard-first workflows first-class.
- Multiple windows or panes are normal for developer tools and terminals.
- Large data sets are virtualized at the viewport boundary; connectors and
  background work feed the view rather than blocking rendering.

What Ferriso should take:

- The actor already provides entity-like lifetime and effect ownership. Keep
  it. Do not import GPUI's `Context` into `App`.
- Add a small action/keymap layer, because raw key matching is the repeated
  translation cost visible in every adapter.
- Treat focus, window, and pane state as adapter-local until an application
  proves it must be shared across frontends.
- Add multi-window support only when a concrete downstream app needs one actor
  and several windows; the core API already permits it, but the adapter API does
  not yet have a lifecycle contract.

### Lapce and Floem

Floem demonstrates fine-grained reactive signals over wgpu. It is a better fit
for redraw-heavy editors than a coarse snapshot channel.

What Ferriso should take:

- Fine-grained reactivity is an adapter implementation detail. A renderer may
  mirror a snapshot into signals; the actor still publishes immutable views.
- Watch-based latest-only delivery is the right default for application state:
  a slow frontend must not replay stale states.
- If an application needs change sets (for example an editor buffer), make them
  application data, not a second state channel hidden in the adapter.

### Helix and helix-tui

Helix contributes modal editing, selection-first commands, and a rope model.
Its TUI-first position is compatible with Ferriso's terminal adapter.

What Ferriso should take:

- Modal editing is application policy; the editor kernel already owns it.
- Key sequences and modes need a shared keymap representation before an editor
  adapter can be reused across TUI and GUI.
- Keep text editing in the editor kernel. The adapters handle IME, preedit, and
  rendering; they do not own selection or undo.

### Iced, COSMIC, and the Iced component ecosystem

The Iced ecosystem (libcosmic, iced_aw, iced_term, iced_video_player,
iced_code_editor, iced_audio, Halloy, Cfait, Furtherance, onagre,
super_clipboard, Airshipper, ajour, microlaunch, Neothesia, Frostbyte,
iced_7guis) shows that a small adapter plus composable widgets scales further
than a monolith.

What Ferriso should take:

- Keep `ferriso-gui` as an adapter, not a widget framework. Re-export `iced` so
  applications can use the ecosystem directly.
- Keep `design` (tokens/styles), `interaction` (palette/toasts), and `art`
  (shader/canvas) as optional, copyable layers.
- Use frame subscriptions for animation and high-frequency visual updates. They
  must never wake the actor.
- Treat terminal integration, video, audio, and editors as opt-in components,
  not mandatory framework dependencies.

### Data, API, terminal, and operations tools

DBFlux, DBDelve, FluxDB, based, dbui, GetCat, arc, Rulogman, Baudrun, and
Farcaster all combine background I/O with a responsive surface.

What Ferriso should take:

- The command/subscription model already covers connection state machines,
  request cancellation, progress, and reconnects.
- Paging and viewport selection are application data. The adapter renders the
  visible slice; it must not receive the whole result set by accident.
- A framework "database" or "HTTP" abstraction would be false reuse. Keep those
  as application crates.

### Other UI stacks

- Gyroflow (Rust core, QML UI), Spacedrive (Tauri + React), RustDesk
  (Sciter/Flutter), and AppFlowy (Flutter + Rust) show productive split-stack
  architectures. They are not evidence for making a C/WebView dependency the
  core of Ferriso.
- Czkawka's GTK frontend and Krokiet's Slint frontend show the same lesson from
  the other direction: one kernel, multiple independent UI shells.
- Rerun (egui/eframe) and Sniffnet (Iced) show that immediate-mode and retained
  Elm-style renderers can both sit on top of a data-oriented core.
- Slint and egui remain valid alternative frontends; Ferriso should not force
  every adapter into Iced's rendering model.

## Project ledger

This is the compact mapping for the supplied list. Repeated entries share a
lesson rather than implying that Ferriso should copy each application.

| Entry | Category | What Ferriso takes |
| --- | --- | --- |
| Zed | Reference / excluded | GPUI parent project; keep it as a benchmark for GPU-native editing, not a dependency or abstraction to clone. |
| Lapce | Editor | Floem + wgpu; confirms fine-grained reactivity belongs inside an adapter. |
| Helix / helix-tui | Editor | Modal, selection-first editing and a reusable keymap model; GUI is still planned. |
| Fulgur | Editor | GPUI workload: text editing, focus, commands, and window lifecycle. |
| Arbor, Codux, Farcaster | GPUI developer tools | Entity-owned async work, terminal panes, Git/diff views, and multi-window needs. |
| GitComet, Rulogman, Baudrun | GPUI tools | Git workflows, SSH, and serial I/O all need background jobs with cancellation and durable view state. |
| GetCat, arc | GPUI API tools | Large responses and cURL import are application data; keep HTTP out of core. |
| DBFlux, DBDelve, FluxDB, based, dbui | GPUI database tools | Virtualized result sets, connection state machines, and SQL editing; viewport policy stays in the app. |
| Gyroflow | Video / QML + Rust | Rust core with a QML shell validates the split, but QML is not Ferriso's required GUI path. |
| Czkawka / Krokiet | Cleanup tools | GTK and Slint shells over one kernel reinforce adapter independence; Krokiet is the pure-Rust frontend option. |
| Spacedrive | File manager / Tauri + React | WebView split-stack; not a reason to put web technology in the framework. |
| RustDesk | Remote desktop / Sciter or Flutter | Non-Rust GUI choices do not block a Rust core; keep them out of the P0 contract. |
| Sniffnet | Network monitor / Iced | Real-time charts and frame refresh fit the renderer-local clock model. |
| Rerun | Visualization / egui + eframe | Immediate-mode renderers can consume data-oriented state; do not force Elm architecture on every adapter. |
| AppFlowy | Knowledge base / Flutter + Rust | Backend/frontend split is valid, but Flutter is not a framework dependency. |
| OctaSine | Audio / Iced | Audio and GUI threads communicate through messages; do not put an audio device in core. |
| iced_7guis, Iced examples | Iced samples | CRUD, timers, and cells show adapter-local state and semantic message patterns. |
| COSMIC Settings, Launcher, App Library, Applets, Text Editor | COSMIC / libcosmic + Iced | Routing, grouped forms, IPC search, virtualized grids, panel lifecycles, and incremental text are application features over an adapter. |
| Halloy, Cfait, Furtherance, onagre, super_clipboard | Iced applications | Message streams, offline-first state, timers, fuzzy search, and background clipboard listeners. |
| Airshipper, ajour, microlaunch | Iced launchers | Download progress and concurrent metadata are command/subscription workloads. |
| Neothesia, Frostbyte Terminal | Iced media/terminal | MIDI-driven animation and PTY integration should remain optional components, not core dependencies. |
| awesome-iced, iced_aw, iced_video_player, iced_code_editor, iced_audio, iced_term | Iced ecosystem | Re-export Iced and compose optional widgets; do not vendor a mandatory widget catalog. |
| libcosmic, GPUI, Floem, Iced, egui/eframe, Slint, wgpu, helix-tui | Frameworks and backends | Compare ownership and rendering models; retain the smallest shared contract. |
| Tauri + React, QML/Qt, GTK 4, Sciter, Flutter | Non-pure-Rust stacks | Useful external comparisons, but excluded from the framework's required dependency path. |

## Capability matrix

`✅` means the capability exists in the project/framework; `◐` means partial or
adapter-specific; `—` means not a stated goal. This is a design comparison, not
a feature-parity score.

| Capability | Ferriso | GPUI cluster | Helix | Iced/COSMIC | egui/Rerun | Slint/Krokiet |
| --- | --- | --- | --- | --- | --- | --- |
| Pure Rust UI path | ✅ | ✅ | ✅ (TUI) | ✅ | ✅ | ✅ |
| One domain owner | ✅ actor | ✅ entities | ✅ kernel | ◐ app-defined | ◐ app-defined | ◐ app-defined |
| TUI + desktop from one model | ✅ | — | ◐ GUI planned | ◐ | — | — |
| GPU-native rendering | ✅ iced; ◐ GPUI/Floem probes | ✅ | — | ✅ | ✅ | ✅ |
| Text/IME as a framework concern | ◐ adapter + editor app | ✅ in GPUI apps | ✅ | ✅ | ✅ | ✅ |
| Virtualized large data | ◐ app policy | ✅ typical | ✅ | ✅ | ✅ | ✅ |
| Frontend-neutral keymaps | ✅ `ferriso-keymap` | ✅ actions | ✅ | ◐ app-defined | ◐ app-defined | ◐ app-defined |
| Multi-window | ◐ core supports handles; adapters are single-window | ✅ | — | ◐ | ✅ | ✅ |
| Async I/O + cancellation | ✅ commands/subscriptions | ✅ | ✅ | ✅ | ✅ | ✅ |
| Diagnostics boundary | ✅ | ◐ app-defined | ◐ | ◐ | ◐ | ◐ |
| Headless test harness | ✅ | ◐ GPUI tests | ✅ TUI | ◐ | ✅ | ◐ |

## Roadmap

### P0: keyboard actions (implemented with this study)

Add `ferriso-keymap`:

- frontend-neutral `Key`, `Modifiers`, `Chord`, and `Keymap<Message>`;
- parser for bindings such as `Ctrl+Shift+P` and `ArrowLeft`;
- exact modifier matching, with case-insensitive plain-character shortcuts;
- adapter converters for Iced and Crossterm;
- first consumers: the player GUI and the 2048 GUI.

Why P0: every keyboard-first project in the list needs this, and the current
code duplicates matching rules in each adapter. It is small, dependency-free,
and testable without a window. This first layer binds one chord per action;
modal prefixes and multi-key sequences remain application-owned until a second
consumer needs them.

### P1: multi-window contract

Design an adapter API for one actor and several native windows. The core should
not change: a second window is another snapshot subscriber plus a message
sender. The open questions are renderer ownership (one renderer per window or a
shared one), focus routing, and close semantics. Do not implement until a
downstream app needs it.

### P1: virtualized collection policy

`ferriso-workspace::TableState` now owns the fixed-row-height viewport,
visible range, selection, and scroll clamping. Applications still own stable
row ids, columns, paging queries, and cell rendering; a framework table widget
would either duplicate GPUI/Iced widgets or force a
lowest-common-denominator layout model.

### P1: animation helpers

The 2048 probe shows the shape: snapshots carry enough change metadata, the
renderer owns an elapsed clock, and iced frame subscriptions stop when the
effect settles. Extract a tiny clock/easing helper only when a second app needs
it; do not put animation state in the actor.

### P2: accessibility semantics

A useful accessibility tree needs labels, roles, values, focus order, and
actions. That is a renderer contract, not a `View` fields problem. Research
Iced/AccessKit and terminal screen-reader behavior before adding anything.

### P2: text/IME conformance fixtures

The editor app already exercises preedit and grapheme navigation. Add shared
fixtures that both TUI and iced adapter tests can drive, so IME and selection
bugs are caught at the boundary rather than in one renderer.

### P2: component growth

`design`, `interaction`, and `art` should grow from real application needs
(command palette, toasts, shader backdrop, chart). Avoid a mandatory widget
catalog; iced_aw/libcosmic are the better ecosystem reference for breadth. The
focused visual/interaction decisions live in
[`art-interaction-study.md`](art-interaction-study.md).

## Non-goals

- No widget tree in `ferriso-core`.
- No database, HTTP, audio, terminal, or video dependency in the core.
- No state duplication to make a renderer convenient.
- No required WebView, Qt, GTK, or Sciter runtime.
- No promise that every frontend has identical features. The contract is the
  semantic interaction path, not pixel parity.
