// The session's desk on the phone: the agent's show_page lands in it, opening the tab keeps the
// chat bar, and a message sent from there reaches the session. Run on the emulator by
// scripts/phone-e2e.sh.

import 'package:composer_app/ui/chat/activity_line.dart';
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

  testWidgets('a page on the desk opens with the chat bar', (tester) async {
    await startPaired(tester);
    await sendInNewChat(tester, 'Use show_page to put site://plans on the desk, then say done.');

    // The turn, and the tab it puts on the desk.
    await pumpUntil(tester, find.byType(ActivityLine), seconds: 30);
    await pumpUntilGone(tester, find.byType(ActivityLine), seconds: 120);
    await tester.tap(find.byKey(const Key('desk')));
    final tab = find.byKey(const Key('desk-tab-page:site://plans'));
    await pumpUntil(tester, tab, seconds: 30);
    await tester.tap(tab);

    // Full screen, the page, and the chat bar still at the bottom.
    await pumpUntil(tester, pageTitled('Plans'), seconds: 30);
    // The chat page below keeps its own composer; this is the desk tab's.
    await tester.enterText(find.byKey(const Key('message')).last, 'from the desk bar');
    await tester.pump();
    await tester.tap(find.byKey(const Key('send')).last);

    // Back in the chat, the message reached the session.
    await tester.binding.handlePopRoute();
    await tester.pumpAndSettle();
    await pumpUntil(tester, richText('from the desk bar'), seconds: 30);
  });
}
