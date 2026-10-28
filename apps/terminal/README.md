# Ferriso Terminal

An app-local embedded terminal prototype for B2: `rio-vt` owns the PTY, VT
parser, grid, and scrollback; this app owns the window adaptation and the iced
cell renderer.

```bash
cargo run -p ferriso-terminal
cargo test -p ferriso-terminal
```

Current scope:

- local shell through a PTY
- ANSI colors and cursor rendering
- window resize updates the terminal grid and PTY size
- keyboard input, including control chords and IME commit
- 100x30 initial grid with 8.4x19 logical cells

Not included yet:

- mouse selection and scrollback navigation
- copy/paste and OSC 52 clipboard integration
- shell-exit status UI
- image protocols and hyperlinks
- Windows ConPTY verification

`rio-vt` is an app-local dependency; it is not part of `ferriso-core`,
`ferriso-gui`, or `ferriso-tui`.
