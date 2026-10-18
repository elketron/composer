// Pairs with a real server through the UI and waits for the connection.
//
//   COMPOSER_APP_DATA=<empty dir> flutter test integration_test/pairing_test.dart -d linux \
//     --dart-define=PAIR_LINE="composer login http://127.0.0.1:7420 AB12-CD34"
//
// (get the line from `composer pair` on the server).

import 'package:composer_app/main.dart' as app;
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

const pairLine = String.fromEnvironment('PAIR_LINE');

Future<void> pumpUntil(WidgetTester tester, Finder finder, {Duration timeout = const Duration(seconds: 20)}) async {
  final end = DateTime.now().add(timeout);
  while (DateTime.now().isBefore(end)) {
    await tester.pump(const Duration(milliseconds: 100));
    if (finder.evaluate().isNotEmpty) return;
    await Future<void>.delayed(const Duration(milliseconds: 100));
  }
  fail('timed out waiting for $finder');
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('pair with a server, then it connects', (tester) async {
    expect(pairLine, isNotEmpty, reason: 'pass --dart-define=PAIR_LINE=...');
    app.main();
    await pumpUntil(tester, find.byKey(const Key('add-first-server')));
    await tester.tap(find.byKey(const Key('add-first-server')));
    await pumpUntil(tester, find.byKey(const Key('server-url')));

    // Pasting the login line fills the address and the code.
    await tester.enterText(find.byKey(const Key('server-url')), pairLine);
    await tester.pump();
    final code = tester.widget<TextField>(find.byKey(const Key('pair-code'))).controller!.text;
    expect(code, pairLine.split(' ').last);

    await tester.tap(find.byKey(const Key('pair')));
    await pumpUntil(tester, find.text('online'));
    expect(find.textContaining('slots 2', findRichText: true), findsOneWidget);
    expect(find.text('SESSIONS'), findsOneWidget);
  });
}
