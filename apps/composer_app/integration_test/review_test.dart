// A finished ticket's changes, reviewed and merged beside the orchestrator's chat.
//
// Needs a paired server in COMPOSER_APP_DATA (run pairing_test.dart first) with an orchestrator
// session whose chat has a worker's report for a ticket with changes on its branch (`ticket/<id>`,
// a worktree in state/worktrees/<id>). Run in headless Weston, wide window:
//
//   COMPOSER_APP_DATA=<dir> flutter test integration_test/review_test.dart -d linux \
//     --dart-define=SESSION=<orchestrator session> --dart-define=TICKET=<id>

import 'package:composer_app/main.dart' as app;
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
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
    await pumpUntil(tester, find.byKey(const Key('session-$session')));
    await tester.tap(find.byKey(const Key('session-$session')).first);

    // ctrl-k: review the ticket's changes (the Changes tab beside the chat).
    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.keyK);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await pumpUntil(tester, find.byKey(const Key('palette-query')));
    await tester.enterText(find.byKey(const Key('palette-query')), 'review $ticket');
    await settle(tester, seconds: 1);
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await pumpUntil(tester, find.textContaining('ticket/$ticket → '));
    final merge = find.byKey(const Key('merge-ticket'));
    await pumpUntil(tester, merge);
    await settle(tester, seconds: 4);

    await tester.tap(merge);
    await pumpUntil(tester, find.textContaining('Merged into'));
    await settle(tester, seconds: 4);
  });
}
