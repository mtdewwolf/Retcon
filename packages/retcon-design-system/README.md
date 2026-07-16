# Retcon Design System

The **Luna Dark** Flutter design system for Retcon. It combines dense,
Windows-XP-inspired desktop chrome with explicit keyboard, screen-reader,
high-contrast, reduced-motion, and large-target behavior.

Use tokens instead of hard-coded visual values and use `Retcon*` controls when
one exists. Every new component must be represented in
`RetconComponentGallery` and covered by a keyboard or semantics test.

```dart
MaterialApp(
  theme: buildLunaDarkTheme(highContrast: false),
  home: const RetconComponentGallery(),
);
```

Accessibility variants are selected with `highContrast`, `reducedMotion`, and
`largeTargets` arguments to `buildLunaDarkTheme`.
