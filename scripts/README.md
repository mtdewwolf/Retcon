# Scripts

Development and CI helper scripts. Conventions:

- Cross-platform scripts are written for POSIX `sh` (run via Git Bash on Windows) or as
  Rust/Dart/TS tools when they need real logic.
- Windows-only helpers may be PowerShell (`.ps1`).
- Every script starts with a comment stating what it does and where it is invoked from
  (developer machine, CI, or both).
