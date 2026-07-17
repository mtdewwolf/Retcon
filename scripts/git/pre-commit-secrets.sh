#!/usr/bin/env bash
# Scan staged Git changes for likely secrets before commit.
# Install: scripts/git/install-pre-commit-hook.sh

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

cargo run --quiet -p retcon-secrets -- scan-staged
