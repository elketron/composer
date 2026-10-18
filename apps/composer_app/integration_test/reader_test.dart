// The reader against a real server: a site from the server page, a micro app in the pane, and
// following the agent when it opens a page. Same setup as chat_test.dart (a paired server in
// COMPOSER_APP_DATA, headless Weston, a wide window); the orchestration site's app must be built.

import 'package:composer_app/main.dart' as app;
import 'package:composer_app/ui/pages/app_view.dart';
import 'package:composer_app/ui/pages/page_body.dart';
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

/// Lets real time pass while painting (and screenshots get taken).
Future<void> settle(WidgetTester tester, {required int seconds}) async {
  for (var i = 0; i < seconds * 5; i++) {
    await tester.pump(const Duration(milliseconds: 100));
    await Future<void>.delayed(const Duration(milliseconds: 100));
  }
}

/// The reader shows `title` (the separate title or the page's own heading).
Finder pageTitled(String title) => find.descendant(
      of: find.byType(PageBody),
      matching: find.byWidgetPredicate((w) => switch (w) {
            Text(:final data) => data == title,
            RichText(:final text) => text.toPlainText() == title,
            _ => false,
          }),
      skipOffstage: false,
    );

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('pages, an app, and following the agent', (tester) async {
    app.main();
    await pumpUntil(tester, find.text('online'));
    final close = find.byTooltip('Close the chat');
    if (close.evaluate().isNotEmpty) await tester.tap(close);

    // A site from the server page opens in the pane.
    await pumpUntil(tester, find.byKey(const Key('site-memory')));
    await tester.tap(find.byKey(const Key('site-memory')));
    await pumpUntil(tester, pageTitled('Memory'));
    await settle(tester, seconds: 3);

    // A micro app in the same pane.
    await tester.tap(find.byKey(const Key('app-orchestration')));
    await pumpUntil(tester, find.byType(AppView));
    await settle(tester, seconds: 6);

    // The agent opens a page; the reader follows.
    await tester.tap(find.byKey(const Key('new-session')));
    await pumpUntil(tester, find.byKey(const Key('message')));
    await tester.enterText(find.byKey(const Key('message')), 'Use your open tool to open site://plans, then reply with just its title.');
    await tester.pump();
    await tester.tap(find.byKey(const Key('send')));
    await pumpUntil(tester, pageTitled('Plans'), seconds: 180);
    expect(find.byType(AppView), findsNothing);
    await settle(tester, seconds: 6);
  });
}
