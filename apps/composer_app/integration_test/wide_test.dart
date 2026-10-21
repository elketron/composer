// The desktop frame against a real server: the latest chat opens by itself, the reader, a site's
// app and a ticket's changes in the side pane's tabs, new work in a pipeline, and folding the
// session list.
//
// Needs a paired server in COMPOSER_APP_DATA (run pairing_test.dart first) whose latest chat is an
// orchestrator with a worker's report for a ticket that still has its branch, and a built
// orchestration app. Run in headless Weston, wide window:
//
//   COMPOSER_APP_DATA=<dir> flutter test integration_test/wide_test.dart -d linux \
//     --dart-define=TICKET=<id> --dart-define=WORKER=<its worker session>

import 'package:composer_app/main.dart' as app;
import 'package:composer_app/ui/pages/app_view.dart';
import 'package:composer_app/ui/wide/session_column.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

const ticket = String.fromEnvironment('TICKET');
const worker = String.fromEnvironment('WORKER');

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

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('the wide frame', (tester) async {
    app.main();
    // The latest chat opens without asking.
    await pumpUntil(tester, find.byKey(const Key('chat-menu')));
    await settle(tester, seconds: 2);

    // The header's chips: the project, and Claude Code off and on again for it.
    await pumpUntil(tester, find.byKey(const Key('project-chip')));
    await pumpUntil(tester, find.byKey(const Key('harness-chip-claude-code')));
    await tester.tap(find.byKey(const Key('harness-chip-claude-code')));
    await pumpUntil(tester, find.text('claude-code off'));
    await settle(tester, seconds: 2);
    await tester.tap(find.byKey(const Key('harness-chip-claude-code')));
    await pumpUntil(tester, find.text('claude-code on'));

    // The project chip opens the project beside the chat: works, branches, sessions.
    await tester.tap(find.byKey(const Key('project-chip')));
    await pumpUntil(tester, find.text('BRANCHES TO MERGE'));
    await settle(tester, seconds: 3);

    // New work from the palette: a video (made from its template the first time), linking this
    // project; ctrl-enter creates it, its page opens beside the chat, and the project lists it
    // with its stages.
    final work = 'teaser-${DateTime.now().millisecondsSinceEpoch % 100000}';
    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.keyK);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await pumpUntil(tester, find.byKey(const Key('palette-query')));
    await tester.enterText(find.byKey(const Key('palette-query')), 'new work');
    await settle(tester, seconds: 1);
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await pumpUntil(tester, find.byKey(const Key('pipeline-video')));
    await tester.tap(find.byKey(const Key('pipeline-video')));
    await tester.enterText(find.byKey(const Key('work-name')), work);
    await tester.enterText(find.byKey(const Key('work-brief')), 'A 20 second teaser for the release.');
    await settle(tester, seconds: 2);
    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await pumpUntilGone(tester, find.byKey(const Key('work-name')));
    await pumpUntil(tester, find.text('A 20 second teaser for the release.'));
    await settle(tester, seconds: 2);
    await tester.tap(find.byKey(const Key('project-chip')));
    await pumpUntil(tester, find.byKey(Key('work-site://video/$work/index.md')));
    await settle(tester, seconds: 2);

    // The scope: … finds a project and narrows the session list to it.
    await tester.tap(find.byKey(const Key('scope-more')));
    await pumpUntil(tester, find.byKey(const Key('project-search')));
    await tester.enterText(find.byKey(const Key('project-search')), 'pro');
    await pumpUntil(tester, find.byKey(const Key('project-proj')));
    await settle(tester, seconds: 2);
    await tester.tap(find.byKey(const Key('project-proj')).last);
    await pumpUntil(tester, find.byKey(const Key('scope-proj')));
    await settle(tester, seconds: 2);

    // The reader in the side pane.
    await tester.tap(find.byKey(const Key('browse')));
    await pumpUntil(tester, find.byKey(const Key('side-tab-page')));
    await settle(tester, seconds: 3);

    // The server in the side pane; its sites; an app beside the chat.
    await tester.tap(find.byKey(const Key('rail-settings')));
    await pumpUntil(tester, find.byKey(const Key('site-orchestration')));
    await settle(tester, seconds: 2);
    await tester.tap(find.byKey(const Key('site-orchestration')));
    await pumpUntil(tester, find.byType(AppView));
    await settle(tester, seconds: 5);

    // Its worker beside the orchestrator (the Worker tab), then as the chat; its changes from ⋮.
    await tester.scrollUntilVisible(
      find.byKey(const Key('worker-line-$worker')),
      200,
      scrollable: find.descendant(of: find.byType(SessionColumn), matching: find.byType(Scrollable)).first,
    );
    await tester.tap(find.byKey(const Key('worker-line-$worker')));
    await pumpUntil(tester, find.byKey(const Key('open-as-chat')));
    await settle(tester, seconds: 3);
    await tester.tap(find.byKey(const Key('open-as-chat')));
    await pumpUntil(tester, find.text('ticket $ticket'));
    await tester.tap(find.byKey(const Key('chat-menu')));
    await pumpUntil(tester, find.text('Review changes of ticket $ticket'));
    await tester.tap(find.text('Review changes of ticket $ticket'));
    await pumpUntil(tester, find.byKey(const Key('merge-ticket')));
    await settle(tester, seconds: 3);

    // Back to the app: still loaded. Then the sessions folded.
    await tester.tap(find.byKey(const Key('side-tab-app')));
    await settle(tester, seconds: 2);
    expect(find.byType(AppView), findsOneWidget);
    await tester.tap(find.byKey(const Key('collapse-sessions')));
    await pumpUntil(tester, find.byKey(const Key('expand-sessions')));
    await settle(tester, seconds: 3);
    await tester.tap(find.byKey(const Key('expand-sessions')));
    await pumpUntil(tester, find.byKey(const Key('collapse-sessions')));

    // esc closes the side pane.
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await settle(tester, seconds: 1);
    expect(find.byKey(const Key('side-close')), findsNothing);

    // ctrl-k: the palette finds the review action; enter runs it.
    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.keyK);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await pumpUntil(tester, find.byKey(const Key('palette-query')));
    await tester.enterText(find.byKey(const Key('palette-query')), 'review $ticket');
    await settle(tester, seconds: 2);
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await pumpUntil(tester, find.byKey(const Key('merge-ticket')));
    await settle(tester, seconds: 1);

    // ctrl-n: the new chat dialog with a first message; esc leaves it.
    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.keyN);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await pumpUntil(tester, find.byKey(const Key('first-message')));
    await settle(tester, seconds: 2);
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await settle(tester, seconds: 1);
    expect(find.byKey(const Key('first-message')), findsNothing);
  });
}
