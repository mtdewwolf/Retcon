# Product screenshot capture and publication gate

The landing page must not ship with conceptual product UI. Publishing requires these three
real captures from the running Windows desktop application:

1. `supervision.webp` — an active session showing task state, attributed commands, changed
   files, and visible stop/cancel control.
2. `approval-review.webp` — a real risky-action approval or complete diff review with enough
   context to make the decision understandable.
3. `browser-verification.webp` — managed Chromium evidence showing the preview, screenshot,
   and visible console/network or test status.

## Capture standard

- Capture at 1600×1000 or larger and export as WebP at 85–90 quality.
- Use the same Windows scaling, Retcon theme, window size, and sample repository in all images.
- Use a purpose-built public fixture repository; never use private customer or personal code.
- Remove usernames, absolute paths, tokens, provider account details, repository remotes,
  notifications, and machine identifiers.
- Show genuine successful states. Do not composite controls, alter results, or claim an
  integration that is not working in the captured build.
- Crop consistently at 16:10. Keep essential UI clear of the lower 15% used by social crops.
- Add no baked-in annotations. Factual captions belong in the website markup for accessibility.

Place approved files in `apps/website/public/assets/product/`, set the repository secret
`TALLY_FORM_ID`, then run `npm run check:launch` from `apps/website`. The GitHub Pages workflow
uses the same gate before deployment.
