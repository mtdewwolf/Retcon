# MVP Scope

This document **freezes** the MVP scope. Anything not listed under "In scope" is out of
scope for the MVP, regardless of how useful it might be.

## In scope (MVP)

The MVP is a single-user Windows desktop application that includes:

- Windows desktop application (Windows 10 and 11)
- Flutter interface with a Windows XP-inspired dark theme
- Rust core service
- Project selection (open a folder / Git repository)
- One coding-agent provider
- Persistent agent sessions
- Integrated terminal
- File tree
- File viewer
- Diff viewer
- Git status
- Git worktrees
- Permission prompts / approvals
- Checkpoints
- Rollback
- External IDE launch
- Managed browser preview (Chromium)
- Screenshot capture
- Basic browser verification
- Crash recovery
- Provider diagnostics ("provider doctor")
- Application logging
- Native Windows installer
- Automatic updates

## Out of scope (MVP)

The following are explicitly excluded from the MVP and deferred to later phases:

- No complete IDE replacement
- No native debugger
- No mobile application
- No team collaboration
- No cloud agent execution
- No marketplace
- No full plugin ecosystem
- No supervisor agent
- No autonomous multi-agent fleet
- No enterprise management console
- No macOS release
- No Linux release
- No custom rendering engine
- No full remote browser streaming
- No built-in deployment marketplace

## Non-goals (durable)

These are not merely "later" — they are things Retcon deliberately will **not** become:

- Retcon will not become an Electron application.
- Retcon will not build a new/custom rendering engine.
- Retcon will not depend on WebView2 for its primary interface.
- Retcon will not require the user to abandon their existing IDE.

## MVP Definition of Done

The MVP is complete when a single user can, on Windows, do all of the following end to end:

1. Install Retcon on Windows
2. Open a Git repository
3. See repository health
4. Configure one agent provider
5. Start an isolated task worktree
6. Ask an agent to implement a change
7. Watch the agent run commands
8. Approve or deny risky actions
9. Inspect changed files
10. Review the complete diff
11. Roll back an agent turn
12. Start the project's development server
13. Open the application in managed Chromium
14. Capture screenshots
15. View console and network errors
16. Run tests and builds
17. Review evidence-based completion
18. Commit approved changes
19. Restart Retcon
20. Resume the previous session
21. Recover from a provider or browser crash
22. Export a diagnostic bundle when something fails

### The defining test

> Retcon must let a user safely delegate a real coding task to an AI agent, inspect
> everything it does, verify the resulting application, and recover cleanly when something
> breaks.

If that sentence is not fully true, the MVP is not done.
