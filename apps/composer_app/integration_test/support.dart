// Shared by the end-to-end tests: waiting on the UI, and pairing with the test server.

import 'package:composer_app/main.dart' as app;
import 'package:composer_app/state/workspace_group.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

/// `composer login <url> <code>` from `composer pair` on the test server; each run needs a new
/// one (scripts/phone-e2e.sh makes it).
const pairLine = String.fromEnvironment('PAIR_LINE');

/// A second workspace of the same host (the pairing test pairs both and switches).
const pairLine2 = String.fromEnvironment('PAIR_LINE_2');

String workspaceInUrl(String url) => Uri.parse(url).pathSegments.last;

/// The URL a `composer login` line opens.
String urlOf(String login) => login.split(' ')[2];

String hostOf(String url) => WorkspaceGroup.hostOf(url);

/// The URLs of the two workspaces the rehearsal server exposes.
(String, String) workspaceUrls() => (urlOf(pairLine), urlOf(pairLine2));

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

/// How many seconds a test stands still at each [shot]; 0 (the default) skips them. A script that
/// watches the test's output takes a screenshot of the emulator at every `SHOT <name>` line.
const shotPause = int.fromEnvironment('SHOT_PAUSE');

/// A moment to photograph: prints `SHOT <name>` and waits [shotPause] seconds (not at all by default).
Future<void> shot(WidgetTester tester, String name) async {
  if (shotPause == 0) return;
  // ignore: avoid_print
  print('SHOT $name');
  await settle(tester, seconds: shotPause);
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
/// Inbox tab shows.
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

/// Adds another workspace through the switcher's sheet, pairing with [login].
Future<void> pairAnother(WidgetTester tester, String login) async {
  await tester.tap(find.byKey(const Key('switch-server')));
  await pumpUntil(tester, find.byKey(const Key('add-workspace')));
  // The sheet is still sliding in when its button first exists.
  await tester.pumpAndSettle();
  await tester.tap(find.byKey(const Key('add-workspace')));
  await pumpUntil(tester, find.byKey(const Key('server-url')));
  await tester.enterText(find.byKey(const Key('server-url')), login);
  await tester.pump();
  await tester.tap(find.byKey(const Key('pair')));
  // It pops back to the switcher once pairing went through.
  await pumpUntilGone(tester, find.byKey(const Key('server-url')), seconds: 30);
  await tester.pumpAndSettle();
}

/// The New chat sheet's Start, tapped once the sheet is up; the chat page is open and empty after.
Future<void> startChat(WidgetTester tester) async {
  await pumpUntil(tester, find.byKey(const Key('start-chat')));
  // The sheet is still sliding in when its button first exists.
  await tester.pumpAndSettle();
  await tester.tap(find.byKey(const Key('start-chat')));
  await pumpUntil(tester, find.byKey(const Key('message')));
  await pumpUntil(tester, find.text('Say something to start.'));
}

/// New chat from the Inbox tab, then [text] sent.
Future<void> sendInNewChat(WidgetTester tester, String text) async {
  await tester.tap(find.byKey(const Key('new-chat')));
  await startChat(tester);
  await tester.enterText(find.byKey(const Key('message')), text);
  await tester.pump();
  await tester.tap(find.byKey(const Key('send')));
}
