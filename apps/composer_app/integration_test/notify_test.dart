// Notifications against a real server: the app goes to the background mid-turn, the recipe
// approval arrives as a notification, its Approve button answers it, and the finished turn
// is notified. Run on a private D-Bus session with scripts/fake-notifyd.py (a daemon that clicks
// Approve), and a `tools` site whose `greet` recipe is not approved yet; see chat_test.dart for
// the rest.

import 'package:composer_app/main.dart' as app;
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

Future<void> pumpUntil(WidgetTester tester, Finder finder, {int seconds = 20}) async {
  final end = DateTime.now().add(Duration(seconds: seconds));
  while (DateTime.now().isBefore(end)) {
    await tester.pump(const Duration(milliseconds: 100));
    if (finder.evaluate().isNotEmpty) return;
    await Future<void>.delayed(const Duration(milliseconds: 200));
  }
  fail('timed out waiting for $finder');
}

Finder anyText(String text) => find.byWidgetPredicate(
      (w) => switch (w) {
        RichText() => w.text.toPlainText().contains(text),
        EditableText() => w.controller.text.contains(text),
        _ => false,
      },
      description: 'text "$text"',
    );

void main() {
  final binding = IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('approve from a notification, then hear the turn finish', (tester) async {
    app.main();
    await pumpUntil(tester, find.byKey(const Key('new-session')));
    await tester.tap(find.byKey(const Key('new-session')));
    await pumpUntil(tester, find.byKey(const Key('start-chat')));
    await tester.tap(find.byKey(const Key('start-chat')));
    await pumpUntil(tester, find.byKey(const Key('message')));
    await tester.enterText(
      find.byKey(const Key('message')),
      'Run the greet recipe of the tools site with the argument bob, then quote exactly what it printed.',
    );
    await tester.pump();
    await tester.tap(find.byKey(const Key('send')));
    await pumpUntil(tester, anyText('Run the greet recipe'));

    // You switch to another window.
    binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);

    // The approval arrives (and is notified); the daemon clicks Approve on the notification,
    // which answers it; the turn goes on and finishes (and is notified).
    final bar = find.byKey(const Key('approval-bar'));
    await pumpUntil(tester, bar, seconds: 240);
    await pumpUntil(tester, find.byWidgetPredicate((w) => w is Text && w.data == 'A recipe needs your approval'));
    final end = DateTime.now().add(const Duration(seconds: 30));
    while (bar.evaluate().isNotEmpty) {
      if (DateTime.now().isAfter(end)) fail('the approval was not answered from the notification');
      await tester.pump(const Duration(milliseconds: 100));
      await Future<void>.delayed(const Duration(milliseconds: 200));
    }
    final idle = find.byWidgetPredicate((w) => w is TextField && w.decoration?.hintText == 'Message');
    await pumpUntil(tester, idle, seconds: 240);
    for (var i = 0; i < 10; i++) {
      await tester.pump(const Duration(milliseconds: 100));
      await Future<void>.delayed(const Duration(milliseconds: 200));
    }
  });
}
