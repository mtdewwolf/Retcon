# Security Policy

Retcon supervises AI agents that run commands and modify code on users' machines. Security
reports are taken seriously and handled with priority.

## Supported versions

Retcon is pre-release. Only the latest commit on the default branch is supported.

## Reporting a vulnerability

**Please do not open public issues for security vulnerabilities.**

Report privately via **GitHub Security Advisories** on this repository
(*Security → Report a vulnerability*). Include:

- A description of the issue and its impact
- Steps to reproduce (a minimal PoC if possible)
- Affected component (desktop shell, core service, browser service, protocol, installer)

You can expect an acknowledgement within 7 days. Please allow a reasonable disclosure
window before publishing details.

## Scope of particular interest

- Permission/approval bypasses (agent actions executing without required approval)
- Secret exposure in logs, diagnostics, artifacts, or provider context
- Local IPC spoofing or privilege escalation between components
- Sandbox/profile escapes in the managed browser service
- Update-chain integrity issues

These follow the project's security principles (least privilege, local-only authenticated
IPC, secret redaction, signed updates) and the threat-model work planned for Phase 32 of
the development plan.
