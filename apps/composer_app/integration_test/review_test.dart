// A finished ticket's changes, reviewed and merged from the orchestrator's chat.
//
// Needs a paired server in COMPOSER_APP_DATA (run pairing_test.dart first) with an orchestrator
// session whose chat has a worker's report for a ticket with changes on its branch (`ticket/<id>`,
// a worktree in state/worktrees/<id>). Run in headless Weston, wide window:
//
//   COMPOSER_APP_DATA=<dir> flutter test integration_test/review_test.dart -d linux \
//     --dart-define=SESSION=<orchestrator session> --dart-define=TICKET=<id>

import 'package:composer_app/main.dart' as app;
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

const session = String.fromEnvironment('SESSION');
const ticket = String.fromEnvironment('TICKET');

Future<void> pumpUntil(WidgetTester tester, Finder finder, {int seconds = 20}) async {
  final end = DateTime.now().add(Duration(seconds: seconds));
  while (DateTime.now().isBefore(end)) {
    await tester.pump(const Duration(milliseconds: 100));
    if (finder.evaluate().isNotEmpty) return;
    await Future<void>.delayed(const Duration(milliseconds: 200));
  }
  fail('timed out waiting for $finder');
}

/// Lets real time pass while painting (and screenshots get taken).
Future<void> settle(WidgetTester tester, {required int seconds}) async {
  for (var i = 0; i < seconds * 5; i++) {
    await tester.pump(const Duration(milliseconds: 100));
    await Future<void>.delayed(const Duration(milliseconds: 100));
  }
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('review and merge a ticket', (tester) async {
    app.main();
    await pumpUntil(tester, find.text('online'));
    await pumpUntil(tester, find.byKey(const Key('session-$session')));
    await tester.tap(find.byKey(const Key('session-$session')).first);

    // The worker's card leads to its changes.
    await pumpUntil(tester, find.byKey(const Key('review-$ticket')));
    await tester.tap(find.byKey(const Key('review-$ticket')));
    await pumpUntil(tester, find.textContaining('ticket/$ticket → '));
    final merge = find.byKey(const Key('merge-ticket'));
    await pumpUntil(tester, merge);
    await settle(tester, seconds: 4);

    await tester.tap(merge);
    await pumpUntil(tester, find.textContaining('Merged into'));
    await settle(tester, seconds: 4);
  });
}
