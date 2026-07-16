# Highest-Risk Areas

These are the technical risks most likely to threaten Retcon, with the mitigations baked
into the roadmap. Documenting them satisfies the Phase 0 gate item "technical risks
documented," and the mitigations shape the sequencing in [milestones.md](milestones.md)
(notably the Phase 2 architecture spike, which exists to retire the top risks early).

## Risk 1 — Flutter terminal performance

Rendering high-volume terminal output smoothly in Flutter is unproven and could make the
integrated terminal feel slow.

**Mitigation:**
- Test the terminal early (in the Phase 2 spike).
- Keep all PTY logic in Rust; Flutter only renders.
- Benchmark large output volumes.
- Keep the terminal renderer replaceable.

## Risk 2 — Flutter code-editor limitations

Flutter is not a proven platform for a full code editor; over-investing here could sink the
MVP.

**Mitigation:**
- Do not build a full editor for the MVP.
- Start with file viewing and lightweight editing only.
- Integrate JetBrains and other external IDEs.
- Revisit a native editor later, if ever.

## Risk 3 — Provider output inconsistency

Different agent providers (and versions) emit inconsistent output, making a uniform UI
hard.

**Mitigation:**
- Build a normalized agent-event model.
- Preserve provider-native data alongside the normalized events.
- Add per-provider capability manifests.
- Test provider versions continuously.

## Risk 4 — Browser embedding

Embedding a browser directly in the desktop shell is risky and can destabilize the UI.

**Mitigation:**
- Use an external managed Chromium window first.
- Keep browser automation in a separate service.
- Embed only after the workflow is stable — never for the MVP.

## Risk 5 — Session desynchronization

The UI's view of a session can drift from the provider's real state (for example, the UI
stuck "running" after the provider finished).

**Mitigation:**
- Persist all state transitions.
- Use sequence-numbered events.
- Reconcile provider state after reconnect.
- Add stale-session detection.

## Risk 6 — Scope creep

The roadmap is large; building too much too early is the most likely way the project fails.

**Mitigation:**
- Freeze the MVP (see [mvp-scope.md](mvp-scope.md)).
- Do not build a full IDE.
- Do not add multiple providers too early.
- Do not build plugins before the core is stable.
- Do not build a custom renderer.
- Do not add cloud infrastructure before local workflows work.
