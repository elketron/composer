// A finished ticket's changes, reviewed and merged from the orchestrator's chat.
//
// Needs a rehearsal server with a session whose chat is the orchestrator's for a ticket with
// changes on its branch (`ticket/<id>`, a worktree in state/worktrees/<id>`):
//
//   flutter test integration_test/review_test.dart -d emulator-5554 \
//     --dart-define=PAIR_LINE="composer login <workspace url> <code>" \
//     --dart-define=SESSION=<orchestrator session> --dart-define=TICKET=<id>
//
// Not in scripts/phone-e2e.sh's default set: it needs that fixture.

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

import 'support.dart';

const session = String.fromEnvironment('SESSION');
const ticket = String.fromEnvironment('TICKET');

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('review and merge a ticket', (tester) async {
    expect(session, isNotEmpty, reason: 'pass --dart-define=SESSION=...');
    expect(ticket, isNotEmpty, reason: 'pass --dart-define=TICKET=...');
    await startPaired(tester);
    await pumpUntil(tester, find.byKey(Key('session-$session')));
    await tester.tap(find.byKey(Key('session-$session')).first);

    // ⋮ in the chat: review the ticket's changes.
    await pumpUntil(tester, find.byKey(const Key('chat-menu')));
    await tester.tap(find.byKey(const Key('chat-menu')));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Review changes of ticket $ticket'));
    await pumpUntil(tester, find.textContaining('ticket/$ticket → '));
    final merge = find.byKey(const Key('merge-ticket'));
    await pumpUntil(tester, merge);
    await settle(tester, seconds: 4);

    await tester.tap(merge);
    await pumpUntil(tester, find.textContaining('Merged into'));
    await settle(tester, seconds: 4);
  });
}
