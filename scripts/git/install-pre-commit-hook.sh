#!/usr/bin/env bash
# Install the Retcon secret-scan pre-commit hook in .git/hooks/pre-commit.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
HOOK="$ROOT/.git/hooks/pre-commit"
SCANNER="$ROOT/scripts/git/pre-commit-secrets.sh"

cat >"$HOOK" <<EOF
#!/usr/bin/env bash
set -euo pipefail
exec "$SCANNER"
EOF

chmod +x "$HOOK" "$SCANNER"
echo "Installed secret scan pre-commit hook at $HOOK"
