import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:retcon_design_system/retcon_design_system.dart';

void main() {
  testWidgets('button activates from the keyboard', (tester) async {
    var activations = 0;
    await tester.pumpWidget(
      MaterialApp(
        theme: buildLunaDarkTheme(),
        home: Scaffold(
          body: RetconButton(
            label: 'Run task',
            autofocus: true,
            onPressed: () => activations++,
          ),
        ),
      ),
    );
    await tester.pump();
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    expect(activations, 1);
  });

  testWidgets('icon actions expose a semantic label and tooltip', (
    tester,
  ) async {
    final semantics = tester.ensureSemantics();
    await tester.pumpWidget(
      MaterialApp(
        theme: buildLunaDarkTheme(),
        home: Scaffold(
          body: RetconIconButton(
            label: 'Open settings',
            icon: Icons.settings,
            onPressed: () {},
          ),
        ),
      ),
    );
    expect(find.bySemanticsLabel('Open settings'), findsWidgets);
    expect(find.byTooltip('Open settings'), findsOneWidget);
    semantics.dispose();
  });

  testWidgets('large-target mode expands icon actions', (tester) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: buildLunaDarkTheme(largeTargets: true),
        home: Scaffold(
          body: RetconIconButton(
            label: 'Close',
            icon: Icons.close,
            onPressed: () {},
          ),
        ),
      ),
    );
    final size = tester.getSize(find.byType(IconButton));
    expect(size.width, greaterThanOrEqualTo(RetconDimensions.accessibleTarget));
    expect(
      size.height,
      greaterThanOrEqualTo(RetconDimensions.accessibleTarget),
    );
  });

  testWidgets('status badges communicate status without color alone', (
    tester,
  ) async {
    final semantics = tester.ensureSemantics();
    await tester.pumpWidget(
      MaterialApp(
        theme: buildLunaDarkTheme(highContrast: true),
        home: const Scaffold(
          body: RetconBadge(
            label: 'Provider offline',
            status: RetconStatus.error,
          ),
        ),
      ),
    );
    expect(find.bySemanticsLabel('error: Provider offline'), findsOneWidget);
    expect(find.byIcon(Icons.error), findsOneWidget);
    semantics.dispose();
  });
}
