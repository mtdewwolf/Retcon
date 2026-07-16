# Product Vision

## Vision statement

AI coding agents can now write, run, and modify code on a developer's machine largely on
their own. The bottleneck is no longer generating changes — it is **trusting** them: seeing
what an agent actually did, approving the risky steps before they happen, verifying that the
result works, and undoing it cleanly when it does not.

Retcon exists to make that trust practical. It is a native desktop application that runs
alongside the AI coding agents a developer already uses, and turns an opaque agent run into
a **supervised, reviewable, and reversible** workflow. Every command an agent runs, every
file it changes, and every action that could affect the system is surfaced, attributable,
and undoable. Work is not "done" because the agent says so — it is done when there is
evidence: passing tests, a clean build, and a screenshot of the running application.

Retcon supervises agents rather than replacing them. It does not try to be a full IDE, a new
model, or a cloud platform. It is the **control room** for delegating real coding tasks to
AI agents on your own machine — provider-neutral, crash-durable, and honest about failure.

## Target user

The primary user is an **individual software developer working on Windows** who:

- Already uses one or more command-line AI coding agents and wants more oversight than a raw
  terminal session provides.
- Works in Git repositories and cares about reviewing diffs before they land.
- Wants to run and visually verify the application an agent just changed.
- Values being able to stop an agent instantly and roll back anything it did.
- Keeps using their own editor/IDE (often JetBrains or VS Code) and does not want Retcon to
  replace it.

Retcon does not assume the user understands any particular provider's internals. Setup
problems are explained and, where safe, repaired without dropping the user into a terminal.

## Primary use cases

1. **Implement a change in isolation.** Open a Git repo, start an agent in a dedicated task
   worktree, ask it to implement an issue, and watch it run commands and edit files without
   touching the main working tree.
2. **Review and roll back.** Inspect exactly what the agent changed, review the full diff
   hunk by hunk, and roll back a single turn — or the whole task — without destroying
   unrelated user changes.
3. **Verify the running application.** Start the project's dev server, open the app in a
   managed Chromium browser, capture screenshots, and surface console and network errors as
   evidence of whether the change actually works.
4. **Approve risky actions deliberately.** When an agent wants to run a destructive command,
   install a package, push to Git, or reach the network, Retcon pauses and asks — with
   enough context to decide.
5. **Recover cleanly.** After a crash of the UI, the core, a provider, or the browser,
   reopen Retcon and resume the previous session with its history and task progress intact.

## Competitive position

- **Versus raw provider CLIs:** Retcon adds supervision, approval, diff review, checkpoints
  and rollback, browser verification, persistent history, and crash recovery — while staying
  provider-neutral and keeping the agent's native session usable.
- **Versus Electron / VS Code-based agent IDEs:** Retcon is a native desktop shell (Flutter),
  not an Electron app and not built on WebView2 for its primary UI. It deliberately does not
  try to be a complete IDE; it integrates with the user's existing editor instead.
- **Versus cloud agent platforms:** Retcon runs locally by default. The user's code, secrets,
  and agent runs stay on their machine; nothing is transmitted to Retcon-operated services.
  Remote and headless execution are later options, not the foundation.

Retcon's defining angle: **native, local-first, provider-neutral supervision with
evidence-based completion and reliable recovery.**
