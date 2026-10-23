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
| workspace apps | HEAD | 0.2.x | core/adapters tests; player, editor, yazi-desktop TUI and iced builds |
| `boxpigma` | `e021c99` | 0.1.x | TUI and iced compile; TUI launch/render/quit smoke |

The pigma pin validates the 0.1 contract, not HEAD. When pigma moves to 0.2, it
must run the same core interaction path against the new revision and record the
pin here. The deprecated tick hooks remain specifically so that migration does
not need to be atomic.

## Release checklist

1. `./scripts/compat-check.sh` — formatting, architecture boundaries, clippy,
   the full workspace tests, and the feature-free platform build.
2. Live TUI/iced checks listed in `docs/compatibility.md`
3. Update this file, `docs/compatibility.md`, and `CHANGELOG.md`
4. Bump the workspace version and update downstream pins deliberately
