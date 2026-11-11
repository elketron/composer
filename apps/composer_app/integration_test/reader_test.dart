// The reader: a site from the Pages tab, a micro app, and following the agent when it opens a
// page. Run on the emulator by scripts/phone-e2e.sh; the orchestration site's app must be built.

import 'package:composer_app/ui/chat/activity_line.dart';
import 'package:composer_app/ui/pages/app_view.dart';
import 'package:composer_app/ui/pages/page_body.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

import 'support.dart';

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
    await startPaired(tester);

    // A site from the Pages tab opens in the reader.
    await tester.tap(find.byKey(const Key('tab-pages')));
    await pumpUntil(tester, find.byKey(const Key('site-memory')));
    await tester.tap(find.byKey(const Key('site-memory')));
    await pumpUntil(tester, pageTitled('Memory'));
    await settle(tester, seconds: 2);
    // The reader's own Back is the page history; leave the route with the system back.
    await tester.binding.handlePopRoute();
    await tester.pumpAndSettle();

    // A site with a built app opens the app.
    final orchestration = find.byKey(const Key('site-orchestration'));
    await tester.dragUntilVisible(orchestration, find.byKey(const PageStorageKey('pages')), const Offset(0, -200));
    await tester.tap(orchestration);
    await pumpUntil(tester, find.byType(AppView));
    await settle(tester, seconds: 5);
    await tester.binding.handlePopRoute();
    await tester.pumpAndSettle();

    // The agent opens a page; the reader follows it.
    await tester.tap(find.byKey(const Key('tab-inbox')));
    await pumpUntil(tester, find.byKey(const Key('new-chat')));
    await sendInNewChat(tester, 'Use your open tool to open site://plans, then say its title.');
    await pumpUntil(tester, find.byType(ActivityLine), seconds: 30);
    await pumpUntilGone(tester, find.byType(ActivityLine), seconds: 60);
    // Pages is an entry of the chat's ⋮ menu.
    await tester.tap(find.byKey(const Key('chat-menu')));
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const Key('pages')));
    await pumpUntil(tester, pageTitled('Plans'));
    await settle(tester, seconds: 2);
  });
}
