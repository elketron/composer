// Workflows on the phone: switch a session into the demo workflow from the chip, see its live view
// appear, resize it with the handle, leave and see it go; the Workflows tab lists the workflow and
// starts a chat in it. Run on the emulator by scripts/phone-e2e.sh against a server with a `demo`
// site (`app: demoapp`), `[applications.demoapp]` and a `[viewer]` pointing at a running wayai.
// SHOT_PAUSE=<seconds> stops at the interesting screens (see support.dart's `shot`).

import 'package:composer_app/ui/chat/live_view.dart';
import 'package:composer_app/ui/store_scope.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

import 'support.dart';

/// How many sessions the demo tile says are in the workflow ("no sessions", "1 session", "3 sessions").
int sessionsInDemo(WidgetTester tester) {
  final text = tester
      .widgetList<Text>(find.descendant(of: find.byKey(const Key('workflow-demo')), matching: find.byType(Text)))
      .map((t) => t.data ?? '')
      .firstWhere((t) => RegExp(r'^(\d+ sessions?|no sessions)$').hasMatch(t));
  return int.tryParse(text.split(' ').first) ?? 0;
}

void main() {
  final binding = IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  // While a watcher photographs the screens (SHOT_PAUSE), real touches (adb input, an injected
  // pinch) reach the app instead of being dropped by the live test binding.
  if (shotPause > 0) binding.shouldPropagateDevicePointerEvents = true;

  testWidgets('the workflow chip, the live view and the Workflows tab', (tester) async {
    await startPaired(tester);
    await shot(tester, 'inbox-empty');

    // A new chat has no workflow, and so no live view.
    await tester.tap(find.byKey(const Key('new-chat')));
    await startChat(tester);
    final chip = find.byKey(const Key('workflow-chip'));
    expect(find.descendant(of: chip, matching: find.text('no workflow')), findsOneWidget);
    expect(find.byKey(const Key('live-view')), findsNothing);
    await shot(tester, 'session-no-workflow');

    // The chip's sheet lists the demo workflow; choosing it enters it.
    await tester.tap(chip);
    final demo = find.byKey(const Key('workflow-pick-demo'));
    await pumpUntil(tester, demo);
    await tester.pumpAndSettle();
    expect(find.byKey(const Key('workflow-leave')), findsNothing, reason: 'nothing to leave yet');
    await shot(tester, 'workflow-sheet');
    await tester.tap(demo);
    await pumpUntil(tester, find.descendant(of: chip, matching: find.text('demo')), seconds: 60);
    await pumpUntil(tester, find.textContaining('Entered demo'), seconds: 30);

    // Its application's live view appears, on the bare viewer page, and loads.
    await pumpUntil(tester, find.byKey(const Key('live-view')), seconds: 30);
    await pumpUntil(tester, find.byType(LiveView));
    expect(find.text('demoapp · live'), findsOneWidget);
    await settle(tester, seconds: 6);
    expect(find.byKey(const Key('live-view-error')), findsNothing);
    final view = find.byKey(const Key('live-view'));
    double height() => tester.getSize(view).height;

    // The page reported its frame size; medium is the height at which the whole screen fits the
    // phone's width, no bands. The labels sit in the handle row, not over the picture.
    final store = StoreScope.read(tester.element(find.byType(LiveView)));
    await pumpUntil(tester, find.byKey(const Key('live-view-app')));
    expect(store.state.liveFrame, isNotNull, reason: 'the bare page reported the size of the screen');
    expect(height(), closeTo(tester.getSize(view).width / store.state.liveFrame!.aspect, 0.5));
    final medium = height();
    expect(find.descendant(of: view, matching: find.text('demoapp · live')), findsNothing);
    expect(find.descendant(of: find.byKey(const Key('live-view-handle')), matching: find.text('view only')), findsOneWidget);
    await shot(tester, 'session-live-medium');

    // The handle: a tap steps to the next size (large: as tall as the layout allows), a drag snaps
    // to the nearest.
    final handle = find.byKey(const Key('live-view-handle'));
    await tester.tap(handle);
    await tester.pumpAndSettle();
    expect(height(), greaterThan(medium));
    await settle(tester, seconds: 2);
    await shot(tester, 'session-live-large');
    await tester.drag(handle, const Offset(0, -900));
    await tester.pumpAndSettle();
    expect(height(), 96);
    await settle(tester, seconds: 2);
    await shot(tester, 'session-live-small');
    await tester.tap(handle);
    await tester.pumpAndSettle();
    expect(height(), medium);

    // Leaving the workflow takes the view away.
    await tester.tap(chip);
    final leave = find.byKey(const Key('workflow-leave'));
    await pumpUntil(tester, leave);
    await tester.pumpAndSettle();
    await tester.tap(leave);
    await pumpUntil(tester, find.descendant(of: chip, matching: find.text('no workflow')), seconds: 30);
    await pumpUntilGone(tester, find.byKey(const Key('live-view')));
    expect(find.byType(LiveView), findsNothing);

    // Back in it, so the Workflows tab has a session to list.
    await tester.tap(chip);
    await pumpUntil(tester, demo);
    await tester.pumpAndSettle();
    await tester.tap(demo);
    await pumpUntil(tester, find.byKey(const Key('live-view')), seconds: 60);
    await tester.binding.handlePopRoute();
    await tester.pumpAndSettle();

    // The Workflows tab: the demo workflow, its application, the session in it.
    await tester.tap(find.byKey(const Key('tab-workflows')));
    await pumpUntil(tester, find.byKey(const Key('workflow-demo')));
    await shot(tester, 'workflows-tab');
    expect(find.text('demoapp'), findsOneWidget);
    // The server may hold sessions from earlier runs: count against what is there now.
    final before = sessionsInDemo(tester);
    expect(before, greaterThanOrEqualTo(1), reason: 'the session just put in it');
    expect(find.byKey(const Key('workflow-plain')), findsNothing);

    // A new chat in it starts in the workflow.
    await tester.tap(find.byKey(const Key('workflow-new-demo')));
    await startChat(tester);
    await pumpUntil(tester, find.descendant(of: chip, matching: find.text('demo')), seconds: 60);
    await pumpUntil(tester, find.byKey(const Key('live-view')), seconds: 30);
    await tester.binding.handlePopRoute();
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const Key('tab-workflows')));
    await pumpUntil(tester, find.byKey(const Key('workflow-demo')));
    expect(sessionsInDemo(tester), before + 1);

    // The Inbox shows each session's workflow beside its project.
    await tester.tap(find.byKey(const Key('tab-inbox')));
    await tester.pumpAndSettle();
    expect(find.text('demo'), findsWidgets);
    await shot(tester, 'inbox-with-workflows');
  });
}
