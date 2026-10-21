// A whole turn through the UI against a real server and model: new session, a message that
// makes the agent run an unapproved recipe, the approval sheet, and the streamed reply.
//
// Needs a paired server in COMPOSER_APP_DATA (run pairing_test.dart first) whose home has a
// `tools` site with `greet name:` printing `zebra-42`. Run in headless Weston, wide window:
//
//   COMPOSER_APP_DATA=<dir> flutter test integration_test/chat_test.dart -d linux

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

/// Text anywhere: plain, rich, or selectable (the markdown draws through an EditableText).
/// Lets real time pass while painting (and screenshots get taken).
Future<void> settle(WidgetTester tester, {required int seconds}) async {
  for (var i = 0; i < seconds * 5; i++) {
    await tester.pump(const Duration(milliseconds: 100));
    await Future<void>.delayed(const Duration(milliseconds: 100));
  }
}

Finder richText(String text) => find.byWidgetPredicate(
      (w) => switch (w) {
        RichText() => w.text.toPlainText().contains(text),
        EditableText() => w.controller.text.contains(text),
        _ => false,
      },
      description: 'text "$text"',
    );

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('a turn with an approval', (tester) async {
    app.main();
    await pumpUntil(tester, find.byKey(const Key('new-session')));

    await tester.tap(find.byKey(const Key('new-session')));
    await pumpUntil(tester, find.byKey(const Key('start-chat')));
    await tester.tap(find.byKey(const Key('start-chat')));
    await pumpUntil(tester, find.byKey(const Key('message')));
    await pumpUntil(tester, find.text('Say something to start.'));

    await tester.enterText(
      find.byKey(const Key('message')),
      'Run the greet recipe of the tools site with the argument bob, then quote exactly what it printed.',
    );
    await tester.pump();
    await tester.tap(find.byKey(const Key('send')));

    // Your message shows up, then the agent asks to run the recipe: in the chat and under Needs
    // you in the session list.
    await pumpUntil(tester, richText('Run the greet recipe'), seconds: 30);
    final approve = find.byWidgetPredicate((w) => w.key is ValueKey<String> && (w.key as ValueKey<String>).value.startsWith('approve-'));
    await pumpUntil(tester, approve, seconds: 240);
    expect(approve, findsNWidgets(2));
    await settle(tester, seconds: 4);
    await tester.tap(approve.last);

    // Both close and the reply quotes the output.
    await pumpUntil(tester, richText('zebra-42'), seconds: 240);
    expect(approve, findsNothing);
    expect(richText('greet'), findsWidgets);
    await settle(tester, seconds: 4);
  });
}
