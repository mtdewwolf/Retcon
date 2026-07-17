#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"
node scripts/protocol/validate.mjs
node scripts/protocol/generate-typescript.mjs
node scripts/protocol/generate-dart.mjs
echo "Protocol codegen complete."
