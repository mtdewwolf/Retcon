# Retcon pre-alpha brand guide

## Brand idea

Retcon is the control room for AI coding agents. The identity combines the safety of a
checkpoint with the reversibility of rewind. It should feel like serious developer tooling
seen through a confident Windows XP-era interface—not parody, pixel art, or nostalgia for its
own sake.

**Primary name:** Retcon  
**Launch qualifier:** Pre-alpha  
**Descriptor:** Supervise and verify AI coding agents.  
**Primary headline:** Put AI coding agents under supervision.

## Logo system

The checkpoint-rewind mark is a reverse arc around a green checkpoint. Use the full-color
mark on dark backgrounds, the dark lockup on light backgrounds, and the monochrome mark only
where color is unavailable.

- Maintain clear space equal to one quarter of the mark's width.
- Minimum digital size: 24 px for the mark; 140 px for the lockup.
- Do not rotate, recolor, stretch, outline, add glow, or separate the checkpoint from the arc.
- Keep “Pre-alpha” as a neighboring label, not as a permanent part of the core wordmark.

Source and export files:

- `retcon-mark.svg` and `retcon-lockup.svg` — primary dark-background variants
- `retcon-mark-mono.svg` — monochrome variant
- `retcon-lockup-dark.svg` — light-background variant
- `retcon-icon-{64,192,512,1024}.png` — raster icon exports

## Luna Dark palette

| Token | Hex | Use |
|---|---:|---|
| Desktop | `#14161F` | Page and desktop background |
| Window | `#1F2330` | Primary panels |
| Surface | `#262B3B` | Raised controls and toolbars |
| Title top | `#2A3F6F` | Active title-bar gradient |
| Title bottom | `#16233F` | Active title-bar gradient |
| Accent | `#3B77BC` | Links, focus, selection |
| Start green | `#3A7D3C` | Primary actions |
| Text | `#E4E6F0` | Primary text |
| Muted | `#9AA2B8` | Supporting text |
| Warning | `#C79A3B` | Approval required |
| Success | `#4E9A51` | Verified and complete |

Use the bevel highlight `#4A5470` on top/left edges and bevel shadow `#10131C` on
bottom/right edges. Status must always include a label or icon; color alone is not enough.

## Typography and interface language

- UI and marketing: Tahoma, then Segoe UI, then system sans-serif.
- Code, timestamps, labels, and metadata: Cascadia Mono, Consolas, then monospace.
- Headlines are compact, direct, and sentence case. Use the retro treatment in chrome,
  title bars, bevels, and dense labels—not by degrading readability.
- Maintain WCAG AA contrast, visible keyboard focus, reduced-motion support, and scalable text.

## Voice and claims

Write with confident, technical specificity. Prefer “Retcon pauses before risky actions” to
“Retcon keeps you safe.” Say “local-first” rather than “fully offline,” because the user's AI
provider may receive context. Mark unshipped capabilities as planned, and never present a
conceptual interface as product evidence.

Approved message pillars:

1. Supervised: commands and risky actions remain visible and controllable.
2. Reviewable: every change is attributable and inspectable.
3. Reversible: checkpoints and rollback make recovery deliberate.
4. Verified: tests, builds, and browser evidence define completion.
5. Local-first: Retcon requires no Retcon-operated cloud service.
