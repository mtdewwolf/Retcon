# Schemas

Source-of-truth schema definitions:

- `protocol/` — the local RPC protocol between the desktop shell, core service, and
  browser service. Rust, Dart, and TypeScript types are **generated** from these (Phase 4);
  hand-written duplicates are not allowed.
- `workflows/` — workflow recipe schemas (Phase 40).
- `plugins/` — plugin manifest schemas (Phase 43).
