// A whole turn through the UI: new chat, a message that makes the agent run an unapproved
// recipe, the approval, and the reply quoting what it printed. The server's home needs a `tools`
// site with `greet name:` printing `zebra-42`; scripts/phone-e2e.sh sets that up with a scripted
// model.

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

import 'support.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('a turn with an approval', (tester) async {
    await startPaired(tester);
    await sendInNewChat(tester, 'Run the greet recipe of the tools site with the argument bob, then quote what it printed.');

    // Your message shows up, then the agent asks to run the recipe, in the chat.
    await pumpUntil(tester, richText('Run the greet recipe'), seconds: 30);
    final approve = find.byWidgetPredicate((w) => w.key is ValueKey<String> && (w.key as ValueKey<String>).value.startsWith('approve-'));
    await pumpUntil(tester, approve, seconds: 60);
    await settle(tester, seconds: 2);
    await shot(tester, 'chat-approval');
    if (shotPause > 0) {
      // For the screenshots: the same approval in the Inbox (count on the tab), then back to the chat.
      await tester.binding.handlePopRoute();
      await tester.pumpAndSettle();
      await shot(tester, 'inbox-needs-you');
      final session = find.byWidgetPredicate((w) => w.key is ValueKey<String> && (w.key as ValueKey<String>).value.startsWith('session-'));
      await tester.tap(session.first);
      await pumpUntil(tester, approve);
    }
    await tester.tap(approve.last);

    // It closes and the reply quotes the output.
    await pumpUntil(tester, richText('zebra-42'), seconds: 60);
    expect(approve, findsNothing);
    await settle(tester, seconds: 2);
    await shot(tester, 'chat-reply');
  });
}
