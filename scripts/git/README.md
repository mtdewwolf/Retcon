# Git secret scanning

Retcon ships a pre-commit hook that scans **staged** changes for likely secrets using the shared `retcon-secrets` crate (regex + entropy heuristics). The same scanner powers `turn.send` prompt checks and future commit RPC (Phase 16).

## Install

```powershell
.\scripts\git\install-pre-commit-hook.ps1
```

```bash
./scripts/git/install-pre-commit-hook.sh
```

## Manual scan

```bash
cargo run -p retcon-secrets -- scan-staged
cargo run -p retcon-secrets -- scan path/to/file.txt
echo "content" | cargo run -p retcon-secrets -- scan-stdin
```

## RPC

Core exposes `secrets.scan` with `{ "text": "..." }` or `{ "texts": ["...", "..."] }` for clients and future git commit workflows.
