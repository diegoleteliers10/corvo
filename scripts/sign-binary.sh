#!/usr/bin/env bash
set -euo pipefail

TARGET="${1:-target/debug/corvo}"

if [ ! -f "$TARGET" ]; then
    echo "Binary not found at $TARGET"
    exit 1
fi

IDENTITY="corvo-dev"
if ! security find-certificate -c "$IDENTITY" >/dev/null 2>&1; then
    IDENTITY="-"
fi

codesign --force --sign "$IDENTITY" --identifier "sh.corvo.corvo" "$TARGET"
echo "Signed $TARGET with identity: $IDENTITY (sh.corvo.corvo)"
