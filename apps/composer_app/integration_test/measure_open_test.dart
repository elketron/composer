// Throwaway measurement (ticket 0053): tap -> first frame on the emulator, long vs short
// session, against a scratch server built from this branch. The app prints its own
// request/render split (MEASURE lines in effect_runner); this prints the test-side total.
//
//   flutter test integration_test/measure_open_test.dart -d emulator-5554 \
//     --dart-define=PAIR_LINE="composer login http://10.0.2.2:7431 XXXX-YYYY" \
//     --dart-define=SESSIONS=e8ae,5464

import 'package:composer_app/main.dart' as app;
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

const pairLine = String.fromEnvironment('PAIR_LINE');
const sessions = String.fromEnvironment('SESSIONS');

Future<void> pumpUntil(WidgetTester tester, Finder finder, {int seconds = 30}) async {
  final end = DateTime.now().add(Duration(seconds: seconds));
  while (DateTime.now().isBefore(end)) {
    await tester.pump(const Duration(milliseconds: 50));
    if (finder.evaluate().isNotEmpty) return;
    await Future<void>.delayed(const Duration(milliseconds: 50));
  }
  fail('timed out waiting for $finder');
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('open chats, measure', (tester) async {
    expect(pairLine, isNotEmpty);
    app.main();
    await pumpUntil(tester, find.byKey(const Key('add-first-server')), seconds: 15).catchError((_) {});
    if (find.byKey(const Key('add-first-server')).evaluate().isNotEmpty) {
      await pumpUntil(tester, find.byKey(const Key('add-first-server')));
      await tester.tap(find.byKey(const Key('add-first-server')));
      await pumpUntil(tester, find.byKey(const Key('server-url')));
      await tester.enterText(find.byKey(const Key('server-url')), pairLine);
      await tester.pump();
      await tester.tap(find.byKey(const Key('pair')));
    }
    for (final id in sessions.split(',')) {
      await pumpUntil(tester, find.byKey(Key('session-$id')));
      await tester.pump(const Duration(seconds: 1)); // settle so earlier frames drew
      final watch = Stopwatch()..start();
      await tester.tap(find.byKey(Key('session-$id')));
      await tester.pump();
      // first content: markdown draws through an EditableText (selectable)
      final content = find.byWidgetPredicate((w) => w is EditableText && w.controller.text.trim().isNotEmpty);
      final end = DateTime.now().add(const Duration(seconds: 30));
      var drawn = false;
      while (DateTime.now().isBefore(end)) {
        await tester.pump(const Duration(milliseconds: 16));
        await Future<void>.delayed(const Duration(milliseconds: 16));
        if (content.evaluate().isNotEmpty) { drawn = true; break; }
      }
      await tester.pump(const Duration(milliseconds: 200));
      debugPrint('MEASURE test $id tap->content ${watch.elapsedMilliseconds}ms (drawn=$drawn)');
      await tester.pageBack();
      await tester.pump(const Duration(seconds: 1));
    }
  });
}
