// Shared by the end-to-end tests: waiting on the UI, and pairing with the test server.

import 'package:composer_app/main.dart' as app;
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

/// `composer login <url> <code>` from `composer pair` on the test server; each run needs a new
/// one (scripts/phone-e2e.sh makes it).
const pairLine = String.fromEnvironment('PAIR_LINE');

Future<void> pumpUntil(WidgetTester tester, Finder finder, {int seconds = 20}) async {
  final end = DateTime.now().add(Duration(seconds: seconds));
  while (DateTime.now().isBefore(end)) {
    await tester.pump(const Duration(milliseconds: 100));
    if (finder.evaluate().isNotEmpty) return;
    await Future<void>.delayed(const Duration(milliseconds: 200));
  }
  fail('timed out waiting for $finder');
}

Future<void> pumpUntilGone(WidgetTester tester, Finder finder, {int seconds = 20}) async {
  final end = DateTime.now().add(Duration(seconds: seconds));
  while (DateTime.now().isBefore(end)) {
    await tester.pump(const Duration(milliseconds: 100));
    if (finder.evaluate().isEmpty) return;
    await Future<void>.delayed(const Duration(milliseconds: 200));
  }
  fail('timed out waiting for $finder to go');
}

/// Lets real time pass while painting (and screenshots get taken).
Future<void> settle(WidgetTester tester, {required int seconds}) async {
  for (var i = 0; i < seconds * 5; i++) {
    await tester.pump(const Duration(milliseconds: 100));
    await Future<void>.delayed(const Duration(milliseconds: 100));
  }
}

/// Text anywhere: plain, rich, or selectable (the markdown draws through an EditableText).
Finder richText(String text) => find.byWidgetPredicate(
      (w) => switch (w) {
        RichText() => w.text.toPlainText().contains(text),
        EditableText() => w.controller.text.contains(text),
        _ => false,
      },
      description: 'text "$text"',
    );

/// Starts the app (a fresh install: no servers) and pairs it with [pairLine]; done once the
/// Chats tab shows.
Future<void> startPaired(WidgetTester tester) async {
  expect(pairLine, isNotEmpty, reason: 'pass --dart-define=PAIR_LINE=...');
  app.main();
  await pumpUntil(tester, find.byKey(const Key('add-first-server')));
  await tester.tap(find.byKey(const Key('add-first-server')));
  await pumpUntil(tester, find.byKey(const Key('server-url')));
  // Pasting the login line is enough: the address and code are read from it.
  await tester.enterText(find.byKey(const Key('server-url')), pairLine);
  await tester.pump();
  expect(tester.widget<Text>(find.byKey(const Key('pair-parsed'))).data, contains(pairLine.split(' ').last));
  expect(find.byKey(const Key('pair-code')), findsNothing);
  await tester.tap(find.byKey(const Key('pair')));
  await pumpUntil(tester, find.byKey(const Key('new-chat')));
}

/// New chat from the Chats tab, then [text] sent.
Future<void> sendInNewChat(WidgetTester tester, String text) async {
  await tester.tap(find.byKey(const Key('new-chat')));
  await pumpUntil(tester, find.byKey(const Key('start-chat')));
  // The sheet is still sliding in when its button first exists.
  await tester.pumpAndSettle();
  await tester.tap(find.byKey(const Key('start-chat')));
  await pumpUntil(tester, find.byKey(const Key('message')));
  await pumpUntil(tester, find.text('Say something to start.'));
  await tester.enterText(find.byKey(const Key('message')), text);
  await tester.pump();
  await tester.tap(find.byKey(const Key('send')));
}
