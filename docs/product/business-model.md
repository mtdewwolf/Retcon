# Business Model

## Source model: fully open source

Retcon is a **fully open-source** project. The desktop application, the Rust core, the
browser service, the protocol, and the eventual plugin/provider SDK are all intended to be
developed in the open under a single license.

Rationale:

- Retcon supervises agents that run commands and modify code on a user's machine. An
  open, auditable codebase is the strongest possible statement of the security and privacy
  posture in [policies.md](policies.md) — users can verify that nothing is exfiltrated.
- Provider-neutrality and an eventual plugin ecosystem (Phase 43) are far easier to build
  and trust with open source and community contribution.

## License

**Recommended: Apache-2.0.**

- Permissive, so individuals and companies can adopt it freely.
- Includes an explicit patent grant, which matters for a tool that integrates many
  third-party providers and will host a plugin ecosystem.
- Compatible with a broad range of downstream use.

Alternatives considered:

- **MIT** — simpler and equally permissive, but no explicit patent grant.
- **GPL-3.0** — strong copyleft; protects against closed forks but discourages some
  commercial adoption and complicates plugin distribution.

> **Open decision.** The final license is confirmed in **Phase 1** when the `LICENSE` file
> is added to the repository. Apache-2.0 is the working recommendation.

## Pricing

**Retcon itself is free. All AI cost is the user's own provider account (bring-your-own-provider).**

- There is no Retcon-side billing, metering, or subscription in the MVP.
- Users supply and pay for their own coding-agent provider (see
  [platforms-and-agents.md](platforms-and-agents.md)). Retcon supervises that provider; it
  does not proxy or resell model access.
- No Retcon-operated cloud service is required to use the product locally.

## Future monetization — open question

Whether and how Retcon is ever monetized (for example: hosted/cloud execution, team
collaboration, an enterprise management console, or paid support) is left as an **explicit
open question**, not a commitment. Those capabilities are already out of MVP scope. Any
future monetization must not compromise the open-source core or the local-first, free,
BYO-provider promise of the base product.
