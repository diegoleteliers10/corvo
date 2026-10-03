#!/bin/sh
# Verifies the extension template still compiles and passes its tests
# as a real workspace member. The template lives outside the workspace
# on purpose, so CI checks it by temporarily copying it into commands/.
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TEMP="commands/_template-check"
BACKUP="$ROOT/.template-check-Cargo.toml.bak"

cd "$ROOT"

cleanup() {
    if [ -f "$BACKUP" ]; then
        mv "$BACKUP" "$ROOT/Cargo.toml"
    fi
    rm -rf "$TEMP"
}
trap cleanup EXIT

cp Cargo.toml "$BACKUP"
cp -R templates/extension "$TEMP"

# Register the copied crate the same way a contributor would.
python3 - "$TEMP" <<'EOF'
import sys

temp = sys.argv[1]
text = open("Cargo.toml").read()
text = text.replace(
    '    "crates/corvo-config",',
    '    "crates/corvo-config",\n    "' + temp + '",',
    1,
)
text = text.replace(
    'corvo-ext = { path = "crates/corvo-ext" }',
    'corvo-ext = { path = "crates/corvo-ext" }\ncorvo-example = { path = "'
    + temp + '" }',
    1,
)
open("Cargo.toml", "w").write(text)
EOF

cargo test -p corvo-example
cargo clippy -p corvo-example --all-targets -- -D warnings

echo "template check passed"
