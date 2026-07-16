# Milestones and Development Order

This is the agreed high-level delivery sequence. It groups the roadmap's 44 phases into
eight milestones. Full per-phase detail lives in the master development plan; this document
is the Phase 0 "initial milestones agreed" artifact and a map, not a restatement of every
task.

| # | Milestone | Phases | Outcome |
|---|-----------|--------|---------|
| 1 | Technical Proof | 0–2 | A disposable prototype proving Flutter, Rust, PTYs, Git, one provider, and managed Chromium work together. |
| 2 | Retcon Foundation | 3–8 | A stable XP-dark desktop shell connected to a durable Rust service. |
| 3 | First Real Agent Workflow | 9–14 | Open a project, start an agent, watch its activity, approve actions, recover the session. |
| 4 | Safe Code Changes | 15–20 | Agent changes can be isolated, reviewed, approved, committed, and rolled back. |
| 5 | Evidence-Based Completion | 21–25 | Plan work, run the project, test it, inspect it in a browser, and prove whether it works. |
| 6 | MVP Release | 26–33 | A stable Windows MVP suitable for private alpha testing. |
| 7 | Product Validation | 34–35 | Tested by real users; prepared for public beta. |
| 8 | Platform Expansion | 36–44 | Grows from an agent supervisor into a complete agent-native development platform (as demand justifies). |

## Sequencing notes

- **Milestone 1 comes first for a reason.** The Phase 2 architecture spike deliberately
  attacks the highest-risk assumptions (see [risks.md](risks.md)) before the full product is
  built. The prototype from Milestone 1 is expected to be **disposable**.
- **The MVP is Milestones 1–6.** Everything in Milestone 8 (multi-provider, multi-agent,
  plugins, GitHub, remote/headless, project memory, recipes) is post-MVP and gated on real
  user validation — consistent with [mvp-scope.md](mvp-scope.md).
- **Order is risk-driven, not feature-driven.** Foundations (durable core, protocol,
  storage, design system, shell) precede agent workflows; safe-change machinery precedes
  evidence-based completion.

## Where to go next

Phase 0 is documentation only. The immediate next step after sign-off is **Phase 1**:
create the repository, the monorepo structure, the Rust/Flutter/Bun tooling, CI, and the
`LICENSE` file (Apache-2.0, per [business-model.md](business-model.md)).
