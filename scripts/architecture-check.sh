#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

failed=0

forbid_dependency() {
    local package=$1
    local pattern=$2
    if cargo tree -p "$package" --edges normal | grep -Eq "$pattern"; then
        echo "architecture: $package must not depend on $pattern" >&2
        cargo tree -p "$package" --edges normal >&2
        failed=1
    fi
}

require_no_dependencies() {
    local package=$1
    local lines
    lines=$(cargo tree -p "$package" --edges normal | wc -l)
    if [ "$lines" -ne 1 ]; then
        echo "architecture: $package must remain dependency-free" >&2
        cargo tree -p "$package" --edges normal >&2
        failed=1
    fi
}

# The core is the domain layer; renderers and workspace models point at it,
# never the other way around.
forbid_dependency ferriso-core 'ferriso-(gui|tui|gpui|floem|workspace)'
forbid_dependency ferriso-gui 'ferriso-tui'
forbid_dependency ferriso-tui 'ferriso-gui'

# These crates are shared by every frontend and must stay toolkit-free.
require_no_dependencies ferriso-keymap
require_no_dependencies ferriso-workspace

# The platform opener is std-only; native services are opt-in features.
if cargo tree -p ferriso-platform --no-default-features --edges normal \
    | grep -Eq 'arboard|rfd|notify|futures-core|tokio'; then
    echo "architecture: ferriso-platform without features must be std-only" >&2
    cargo tree -p ferriso-platform --no-default-features --edges normal >&2
    failed=1
fi

if [ "$failed" -ne 0 ]; then
    exit 1
fi

echo "architecture boundaries: ok"
