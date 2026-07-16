# Principles

These principles are binding constraints on Retcon's design. When a trade-off arises, these
statements decide it.

## Product principles

- **Explain failures clearly.** Every error has a user-facing message, a cause, and a
  suggested next step. Retcon never fails silently or with only a stack trace.
- **Never hide pending approvals.** Any action awaiting the user's decision is always
  visible and countable. An agent cannot proceed on a risky action without an explicit
  answer.
- **Survive crashes without losing sessions.** Session history, task progress, and layout
  survive a crash of the UI, the core, a provider, or the browser.
- **Make agent changes reviewable.** The user can always see exactly what an agent changed,
  attributed to a session, task, and command, down to the hunk.
- **Provide evidence before declaring work complete.** Completion means tests, builds, and
  browser checks — not the agent's own assertion that it finished.
- **Keep browser automation isolated from the desktop shell.** The managed Chromium runs in
  a separate service and profile; a browser crash cannot take down the UI.
- **Allow users to stop any agent immediately.** Stop and cancel are always available and
  always work, including killing the full child-process tree.
- **Do not require the user to understand provider internals.** Retcon normalizes provider
  behavior and repairs common setup problems without demanding terminal expertise.
- **Work with external IDEs.** Retcon opens files, worktrees, and diff targets in the user's
  preferred editor and stays useful without a full built-in editor.
- **Remain useful without a full built-in editor.** File viewing and lightweight editing are
  enough for the MVP; deep editing happens in the external IDE.

### Architectural non-negotiables

- **Not an Electron application.**
- **No dependency on WebView2 for the primary interface.**
- **No attempt to build a new rendering engine.**

## Security principles

- **Least privilege.** Each process runs with the minimum privileges it needs; child
  processes are tracked and cleaned up.
- **Local-only, authenticated IPC.** The Flutter UI, Rust core, and browser service
  communicate over authenticated local transports (named pipes on Windows) with local-only
  access checks — never over an open network port by default.
- **Redact secrets before they leave the machine.** Context is scanned and secrets are
  redacted before transmission to any provider; diffs are scanned before commit and push.
- **Signed, verifiable updates.** Updates are code-signed, checksum-verified, and can roll
  back on failure.
- **Isolated browser profiles.** Each browser session uses an isolated profile; automation
  and observation cannot leak between sessions or into the desktop shell.
- **Auditable approvals and rules.** Every approval decision and persistent permission rule
  is recorded and revocable; the audit trail is protected.
- **No secrets in logs or diagnostic bundles.** Logs, support bundles, and artifacts are
  redacted; sensitive values live in the OS credential vault.

## User-experience principles

- **XP-inspired dark shell, consistently applied.** A cohesive Windows XP-inspired dark
  ("Luna Dark") theme with a defined palette, typography, bevels, and window states.
- **Keyboard-navigable.** The shell is fully usable without a mouse; commands are reachable
  through menus and a command palette.
- **Never leave the UI stuck.** The interface never remains in a "running" state after a
  provider has actually finished; stale state is detected and reconciled.
- **Transparent cost and context.** Token usage, cost, context usage, and pending approvals
  are visible during a session, not hidden.
- **Accessible by default.** High-contrast theme, screen-reader semantics, adjustable font
  scaling, reduced-motion, color-independent status indicators, and accessible diff and
  terminal controls are part of the baseline, not an afterthought.
- **Layout that persists and recovers.** Panels can be rearranged freely; layouts persist
  per project and recover after crashes or display changes.
