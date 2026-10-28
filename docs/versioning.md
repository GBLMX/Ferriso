# Versioning

Ferriso uses one version for the workspace. `ferriso-core`, the adapters, the
support crates, and the workspace applications are released together; their
public contracts are expected to move in lockstep.

## Semver policy while 0.x

- **Patch** (`0.y.z` to `0.y.z+1`): compatible fixes and documentation.
- **Minor** (`0.y` to `0.y+1`): may contain breaking contract changes. The
  changelog states the migration.
- **Major** (`1.0` and later): stable semver; breaking changes require a major
  release.

A breaking change includes a changed `App` method signature, changed
`Renderer` behavior, changed panic/lifecycle semantics, or a changed snapshot
contract. Adding a defaulted method is compatible unless it changes an existing
application's behavior.

The workspace is not published to crates.io yet. Downstreams that pin a Git
revision are consuming a snapshot of the contract, not a released version.

## Supported downstreams

| Consumer | Ferriso revision | Version line | Verified paths |
| --- | --- | --- | --- |
| workspace apps | HEAD | 0.3.x | core/adapters tests; player, data-workbench, editor, yazi-desktop TUI and iced builds; twozero48 iced build; ferriso-cli catalog tests |
| `boxpigma` | `55d1745` | 0.3.1 | workspace tests, TUI launch/render/quit, iced window open/close |

`boxpigma` is a same-author downstream fork of pigma. It now consumes the 0.3.1
contract and has migrated its actor timer to `Subscription::interval`; it no
longer uses the deprecated tick hooks. The hooks
remain available for older 0.1/0.2 consumers that have not migrated yet.

## Release checklist

1. `./scripts/compat-check.sh` — formatting, locked dependency resolution,
   architecture boundaries, warning-free rustdoc, clippy, the full workspace
   tests, and the feature-free platform build. CI also checks the declared
   `rust-version = "1.88"` with a locked workspace check.
2. Live TUI/iced checks listed in `docs/compatibility.md`
3. Update this file, `docs/compatibility.md`, and `CHANGELOG.md`
4. Bump the workspace version and update downstream pins deliberately
