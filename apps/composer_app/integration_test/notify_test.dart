// Notifications against a real server: the app goes to the background mid-turn, the recipe
// approval arrives as a notification, its Approve button answers it, and the finished turn
// is notified. Run on a private D-Bus session with scripts/fake-notifyd.py (a daemon that clicks
// Approve, and for the tap test FAKE_CLICK=Open clicks the notification itself), and a `tools`
// site whose `greet` recipe is not approved yet; see chat_test.dart for the rest.
//
//   COMPOSER_APP_DATA=<dir> flutter test integration_test/notify_test.dart -d linux \
//     --dart-define=PAIR_LINE="composer login http://127.0.0.1:7432 XXXX-YYYY" \
//     --plain-name 'a tap on a notification'

import 'package:composer_app/main.dart' as app;
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

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

Finder anyText(String text) => find.byWidgetPredicate(
      (w) => switch (w) {
        RichText() => w.text.toPlainText().contains(text),
        EditableText() => w.controller.text.contains(text),
        _ => false,
      },
      description: 'text "$text"',
    );

/// The app bar line of the chat on screen ends with `#<session>` (its project is whatever the
/// server has, so the id is what the test can key on).
String onScreenChat(WidgetTester tester) {
  for (final t in tester.widgetList<Text>(find.byType(Text))) {
    final m = RegExp(r' #[0-9a-f]+$').firstMatch(t.data ?? '');
    if (m != null) return m.group(0)!.trim();
  }
  fail('no chat page on screen');
}

Finder anyChatLine() => find.byWidgetPredicate((w) => w is Text && RegExp(r' #[0-9a-f]+$').hasMatch(w.data ?? ''));

/// A pop leaves its route on screen until its animation ends: let it finish before the next tap.
Future<void> settle(WidgetTester tester) async {
  for (var i = 0; i < 6; i++) {
    await tester.pump(const Duration(milliseconds: 100));
  }
}

void main() {
  final binding = IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  // Skips without --dart-define=PAIR_LINE=... : it needs an emulator/phone that can complete
  // pairing (headless Weston can't), so a bare suite run skips this leg instead of failing it.
  testWidgets('a tap on a notification opens the chat it came from', (tester) async {
    // A phone: chats open as pages, which is what the tap has to land on. The headless Weston
    // window is 400x720 (its output is phone-wide, and only what fits inside the window gets
    // taps), so the shape stays inside that.
    await tester.binding.setSurfaceSize(const Size(400, 720));
    app.main();
    // Fresh COMPOSER_APP_DATA: pair through the UI first (a one-time code from `composer pair`).
    await pumpUntil(tester, find.byKey(const Key('add-first-server')));
    await tester.tap(find.byKey(const Key('add-first-server')));
    await pumpUntil(tester, find.byKey(const Key('server-url')));
    await tester.enterText(find.byKey(const Key('server-url')), pairLine);
    await tester.pump();
    await tester.tap(find.byKey(const Key('pair')));
    await pumpUntil(tester, find.byKey(const Key('new-chat')), seconds: 30);
    expect(find.byKey(const Key('switch-server')), findsOneWidget, reason: 'the phone layout, or the pop the tap does is not exercised');

    // Chat A, the one you are standing on; then chat B, the one that finishes a turn and notifies.
    // (The phone sheet has no first-message field: the chat starts empty and you type in it.)
    await tester.tap(find.byKey(const Key('new-chat')));
    await pumpUntil(tester, find.byKey(const Key('start-chat')));
    await tester.tap(find.byKey(const Key('start-chat')));
    await pumpUntil(tester, anyChatLine());
    final chatA = onScreenChat(tester);

    await tester.pageBack();
    await settle(tester);
    await tester.tap(find.byKey(const Key('new-chat')));
    await pumpUntil(tester, find.byKey(const Key('start-chat')));
    await tester.tap(find.byKey(const Key('start-chat')));
    await pumpUntil(tester, anyChatLine());
    final chatB = onScreenChat(tester);
    expect(chatB, isNot(chatA), reason: 'two different chats');
    await tester.enterText(
      find.byKey(const Key('message')),
      'Count from 1 to 40, one number per line. End with the word zebra42 alone on the last line.',
    );
    await tester.tap(find.byKey(const Key('send')));

    // Back to A, and away: the notification for B's finished turn is what brings you to B.
    await tester.pageBack();
    await settle(tester);
    await tester.tap(find.byKey(Key('session-$chatA')));
    await pumpUntil(tester, anyText('#$chatA'));
    expect(onScreenChat(tester), chatA, reason: 'standing on A when B notifies');
    binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);

    // fake-notifyd presses the notification (FAKE_CLICK=Open) a couple of seconds after it lands.
    await pumpUntil(tester, anyText('#$chatB'), seconds: 240);
    expect(onScreenChat(tester), chatB, reason: 'the app landed on B, not stayed on A');
    expect(find.textContaining('zebra42'), findsWidgets, reason: 'B shows its finished turn');
  }, skip: pairLine.isEmpty);

  testWidgets('approve from a notification, then hear the turn finish', (tester) async {
    app.main();
    await pumpUntil(tester, find.byKey(const Key('new-session')));
    await tester.tap(find.byKey(const Key('new-session')));
    await pumpUntil(tester, find.byKey(const Key('start-chat')));
    await tester.tap(find.byKey(const Key('start-chat')));
    await pumpUntil(tester, find.byKey(const Key('message')));
    await tester.enterText(
      find.byKey(const Key('message')),
      'Run the greet recipe of the tools site with the argument bob, then quote exactly what it printed.',
    );
    await tester.pump();
    await tester.tap(find.byKey(const Key('send')));
    await pumpUntil(tester, anyText('Run the greet recipe'));

    // You switch to another window.
    binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);

    // The approval arrives (and is notified); the daemon clicks Approve on the notification,
    // which answers it; the turn goes on and finishes (and is notified).
    final approval = find.byWidgetPredicate((w) => w.key is ValueKey<String> && (w.key as ValueKey<String>).value.startsWith('approve-'));
    await pumpUntil(tester, approval, seconds: 240);
    final end = DateTime.now().add(const Duration(seconds: 30));
    while (approval.evaluate().isNotEmpty) {
      if (DateTime.now().isAfter(end)) fail('the approval was not answered from the notification');
      await tester.pump(const Duration(milliseconds: 100));
      await Future<void>.delayed(const Duration(milliseconds: 200));
    }
    final idle = find.byWidgetPredicate((w) => w is TextField && w.decoration?.hintText == 'Message');
    await pumpUntil(tester, idle, seconds: 240);
    for (var i = 0; i < 10; i++) {
      await tester.pump(const Duration(milliseconds: 100));
      await Future<void>.delayed(const Duration(milliseconds: 200));
    }
  });
}
