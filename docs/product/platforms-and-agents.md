# Supported Platforms and Agents

## Operating systems

### Initial (MVP)

- **Windows 10**
- **Windows 11**

The MVP targets Windows exclusively. The desktop shell uses a frameless custom-title-bar
window that must work correctly across Windows 10 and 11, including high-DPI displays,
multiple monitors, snap layouts, and sleep/resume.

### Later (deferred)

These are explicitly out of MVP scope and sequenced for after product validation:

- **Linux** (Unix domain socket transport already anticipated in the protocol design)
- **macOS**
- **Web monitoring interface** (read-mostly remote monitoring)
- **Mobile companion application**

## Coding agents

### MVP: one provider

The MVP ships with exactly **one** coding-agent provider adapter, proven end to end:
installation detection, version detection, authentication detection, session start,
streaming output, tool/approval parsing, session resume, cancellation, failure
normalization, and usage extraction.

> **Open decision — first provider not yet named.** The specific provider that ships first
> is still TBD. Everything below is provider-neutral by design, so the choice can be made
> without reworking the framework. This document will be updated to name the provider once
> the user selects it.

### Provider strategy

Retcon is **provider-neutral**. Rather than binding to one agent's behavior, it defines:

- A **common provider interface** (start/resume/stop session, send message, cancel turn,
  approve/reject, read usage and context, link the native session).
- A **normalized agent-event model** so different providers' output maps to the same set of
  UI events (turn started, text delta, tool requested/started/output/completed, approval
  requested, file changed, command started/completed, usage updated, turn completed,
  provider failed/disconnected, etc.).
- A **capability manifest** per provider (file access/editing, shell, browser/computer use,
  MCP, image input, plan/queue modes, session resume, subagents, model selection, cost and
  token reporting, native approvals) so unsupported capabilities are shown clearly rather
  than failing mysteriously.

Multi-provider support, provider comparison, and model routing are **Phase 36** — after the
first provider is stable. Adding a provider later must not require changing the shared
framework.

### Bring-your-own-provider

The user supplies their own agent CLI and their own account/authentication. Retcon detects,
supervises, and normalizes it — it does not resell, proxy, or bill for provider access. All
AI usage cost is incurred on the user's own provider account. See
[business-model.md](business-model.md).
