// Pairs with a real server through the UI and waits for the connection. Run on the emulator by
// scripts/phone-e2e.sh (it makes the pairing code).

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

import 'support.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('pair with a server, then it connects', (tester) async {
    await startPaired(tester);
    // The Server tab says it is online.
    await tester.tap(find.byKey(const Key('tab-server')));
    await pumpUntil(tester, find.text('online'));
    expect(find.textContaining(' in use'), findsOneWidget);
  });
}
