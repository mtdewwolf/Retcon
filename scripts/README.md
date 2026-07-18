# Scripts

Development and CI helper scripts. Conventions:

- Cross-platform scripts are written for POSIX `sh` (run via Git Bash on Windows) or as
  Rust/Dart/TS tools when they need real logic.
- Windows-only helpers may be PowerShell (`.ps1`).
- Every script starts with a comment stating what it does and where it is invoked from
  (developer machine, CI, or both).

## Repository verification

From the repository root, run the same required inventory used by CI:

```text
node scripts/verify.mjs
```

The command checks protocol generation, license metadata, Rust, all Flutter apps/packages,
the browser service, the website, and the Windows release build when running on Windows. It
buffers successful tool output into a concise pass/fail summary and prints full output for
failures. For a focused run, use `--suite protocol`, `rust`, `flutter`, `browser`, `website`,
or `windows-build`. `node scripts/verify.mjs --list` prints the canonical automated and manual
inventory; `--list --json` is available to developer tools such as the test dashboard.

## Phase 2 platform matrix

See [`phase2/README.md`](phase2/README.md) for the six validation scripts and
[`docs/phase-2-validation.md`](../docs/phase-2-validation.md) for captured evidence.
