// Pairs with a real server through the UI and sees it online. Run on the emulator by
// scripts/phone-e2e.sh (it makes the pairing code).

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

import 'support.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('a server, paired', (tester) async {
    await startPaired(tester);
    await tester.tap(find.byKey(const Key('tab-server')));
    await pumpUntil(tester, find.textContaining('online'));
    expect(find.textContaining(' in use'), findsOneWidget);
    await tester.tap(find.byKey(const Key('tab-inbox')));
  });
}
