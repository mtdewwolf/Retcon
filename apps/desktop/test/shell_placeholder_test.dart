import 'package:flutter_test/flutter_test.dart';
import 'package:retcon_desktop/main.dart';

void main() {
  testWidgets('shell placeholder renders the Retcon window and taskbar', (
    tester,
  ) async {
    await tester.pumpWidget(const RetconApp());

    expect(find.text('Retcon'), findsWidgets);
    expect(find.text('start'), findsOneWidget);
    expect(find.text('Supervise and verify AI coding agents'), findsOneWidget);
  });
}
