# Protocol codegen

Run from the repository root:

```powershell
.\scripts\protocol\generate-all.ps1
```

```bash
./scripts/protocol/generate-all.sh
```

This validates `schemas/protocol/v1.json`, refreshes generated Dart/TypeScript clients, and should be run whenever the schema changes. CI runs the same validation and checks that generated outputs are committed.
