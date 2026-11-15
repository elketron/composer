// Scrolling through a long chat never shows its messages out of order: older pages load in as you
// go up (paged like the server's `session.history`), turns fold their steps into one row, and the
// rows on screen always read oldest to newest — also with messages that scroll on their own (a
// wide code block), whose scrolling is not the chat's. Real reducer, real widgets, no server.

import 'dart:async';

import 'package:composer_app/flux/store.dart';
import 'package:composer_app/state/actions.dart';
import 'package:composer_app/state/app_state.dart';
import 'package:composer_app/state/effects.dart';
import 'package:composer_app/state/reduce.dart';
import 'package:composer_app/state/server_entry.dart';
import 'package:composer_app/ui/chat/chat_view.dart';
import 'package:composer_app/ui/store_scope.dart';
import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

const home = ServerEntry(url: 'https://home.ts.net', name: 'home', token: 't1');

/// A transcript of `turns` turns: the user asks, the agent calls a tool or two, then answers.
/// Message texts are `msg <n>` (n in transcript order) and vary in height.
List<ServerMessage> transcript(int turns) {
  final events = <ServerMessage>[];
  var seq = 0;
  var n = 0;
  String text() {
    n++;
    final lines = 1 + (n * 7) % 5;
    // Every fourth one carries a code block too wide for the screen: it scrolls on its own.
    final code = n % 4 == 0 ? '\n\n```\n${'let wide = ${'x' * 160};\n' * 3}```' : '';
    return 'msg ${n.toString().padLeft(3, '0')}${'\n\nmore words on another line' * lines}$code';
  }

  for (var t = 0; t < turns; t++) {
    events.add(ServerMessageMessageDone(session: 'a', seq: ++seq, message: ChatMessage(role: Role.user, content: text())));
    for (var c = 0; c < 1 + t % 2; c++) {
      final id = 'c$t-$c';
      events.add(ServerMessageToolCall(session: 'a', seq: ++seq, callId: id, name: 'open', args: {'url': 'site://memory/x$t.md'}));
      events.add(ServerMessageToolResult(session: 'a', seq: ++seq, callId: id, result: 'ok', isError: false));
    }
    events.add(ServerMessageMessageDone(session: 'a', seq: ++seq, message: ChatMessage(role: Role.assistant, content: text())));
  }
  return events;
}

/// What the server's `Transcript::history` answers: the last `messages` messages before `before`
/// (cut at a `message.done`), and whether there are more.
(List<ServerMessage>, bool) page(List<ServerMessage> all, int? before, int messages) {
  final events = [for (final e in all) if (before == null || _seq(e) < before) e];
  final starts = [for (final (i, e) in events.indexed) if (e is ServerMessageMessageDone) i];
  final cut = starts.length > messages ? starts[starts.length - messages] : 0;
  return (events.sublist(cut), cut > 0);
}

int _seq(ServerMessage e) => switch (e) {
      ServerMessageMessageDone(:final seq) || ServerMessageToolCall(:final seq) || ServerMessageToolResult(:final seq) => seq,
      _ => 0,
    };

void main() {
  group('newest at the bottom', () => suite(newestFirst: false));
  group('newest first', () => suite(newestFirst: true));
}

void suite({required bool newestFirst}) {
  final away = newestFirst ? -1.0 : 1.0;
  final all = transcript(60);

  /// Bumped to rebuild the chat, as the app does all the time (status, live view, workers).
  final tick = ValueNotifier(0);

  Future<AppStore> openChat(WidgetTester tester) async {
    await tester.binding.setSurfaceSize(const Size(400, 800));
    late AppStore store;
    store = Store(AppState.initial, reduce, (effect, dispatch) {
      if (effect is FetchHistory) {
        final (events, more) = page(all, effect.before, 20);
        scheduleMicrotask(() => store.dispatch(HistoryLoaded(home.url, 'a', effect.before, events, more: more)));
      }
    });
    store.dispatch(const ServersLoaded([home]));
    store.dispatch(SessionOpened(home.url, 'a'));
    await tester.pumpWidget(
      StoreScope(
        store: store,
        child: MaterialApp(
          home: Scaffold(
            body: ValueListenableBuilder(
              valueListenable: tick,
              builder: (context, _, _) {
                final status = StoreScope.of(context).state.statusOf(home.url);
                return ChatView(url: home.url, status: status, chat: status.chat!, newestFirst: newestFirst);
              },
            ),
          ),
        ),
      ),
    );
    await tester.pump();
    await tester.pump();
    return store;
  }

  /// The message numbers on screen, from the top of the screen down.
  List<int> onScreen(WidgetTester tester) {
    final shown = <(double, int)>[];
    final seen = <int>{};
    String text(Widget w) => switch (w) {
          RichText(:final text) => text.toPlainText(),
          EditableText(:final controller) => controller.text,
          _ => '',
        };
    for (final element in [...find.byType(RichText).evaluate(), ...find.byType(EditableText).evaluate()]) {
      final match = RegExp(r'^msg (\d{3})').firstMatch(text(element.widget));
      final box = element.renderObject as RenderBox?;
      if (match == null || box == null || !box.attached) continue;
      final y = box.localToGlobal(Offset.zero).dy;
      if (y < 0 || y > 800) continue;
      final n = int.parse(match.group(1)!);
      if (seen.add(n)) shown.add((y, n));
    }
    shown.sort((a, b) => a.$1.compareTo(b.$1));
    return [for (final (_, n) in shown) n];
  }

  void expectInOrder(WidgetTester tester, String when) {
    final shown = onScreen(tester);
    expect(shown, isNotEmpty, reason: '$when: messages on screen');
    final sorted = [...shown]..sort();
    expect(shown, newestFirst ? sorted.reversed.toList() : sorted, reason: '$when: $shown');
  }

  testWidgets('a message scrolling on its own is not the chat scrolling', (tester) async {
    final store = await openChat(tester);
    await tester.dragFrom(const Offset(4, 400), Offset(0, 600 * away));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 50));
    expect(store.state.statusOf(home.url).chat!.atBottom, isFalse, reason: 'scrolled up');
    // A wide code block's own scrollable reports itself at its start (which, for the chat,
    // would read as "back at the newest line").
    final list = find.byType(Scrollable).first;
    final inner = tester.element(find.descendant(of: list, matching: find.byType(Scrollable)).first);
    ScrollStartNotification(
      metrics: FixedScrollMetrics(minScrollExtent: 0, maxScrollExtent: 0, pixels: 0, viewportDimension: 20, axisDirection: AxisDirection.down, devicePixelRatio: 1),
      context: inner,
    ).dispatch(inner);
    await tester.pump();
    await tester.pump();
    expect(store.state.statusOf(home.url).chat!.atBottom, isFalse, reason: 'still scrolled up');
  });

  testWidgets('a drag that starts on a message scrolls the chat', (tester) async {
    await openChat(tester);
    // A selectable text is a text field with its own vertical scrollable: it can take the drag
    // and, having nothing to scroll, freeze the chat. Messages are plain text the list selects.
    final list = find.byType(Scrollable).first;
    expect(find.descendant(of: list, matching: find.byType(EditableText)), findsNothing);
    final vertical = find.descendant(of: list, matching: find.byWidgetPredicate((w) => w is Scrollable && axisDirectionToAxis(w.axisDirection) == Axis.vertical));
    expect(vertical, findsNothing, reason: 'no message scrolls up and down on its own');
    final pos = tester.state<ScrollableState>(list).position;
    for (var step = 0; step < 6; step++) {
      final before = pos.pixels;
      await tester.dragFrom(const Offset(200, 400), Offset(0, 300 * away));
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 50));
      expect(pos.pixels, isNot(before), reason: 'drag $step moved the chat');
    }
  });

  testWidgets('scrolling up through older pages and back keeps the order', (tester) async {
    final store = await openChat(tester);
    expectInOrder(tester, 'opened');
    for (var step = 0; step < 400 && !onScreen(tester).contains(1); step++) {
      await tester.dragFrom(const Offset(4, 400), Offset(0, 260 * away));
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 50));
      expectInOrder(tester, 'up $step');
    }
    expect(store.state.statusOf(home.url).chat!.more, isFalse, reason: 'it reached the first message');
    expect(onScreen(tester), contains(1), reason: 'the first message is on screen');
    for (var step = 0; step < 40; step++) {
      await tester.flingFrom(const Offset(4, 400), Offset(0, -400 * away), 1500);
      await tester.pump(const Duration(milliseconds: 16));
      expectInOrder(tester, 'down $step');
      await tester.pumpAndSettle();
      expectInOrder(tester, 'down $step settled');
    }
    expect(onScreen(tester), contains(120), reason: 'back at the newest message: ${onScreen(tester)}');
  });

  testWidgets('from the first message, scrolling back step by step reaches the newest', (tester) async {
    await openChat(tester);
    for (var step = 0; step < 400 && !onScreen(tester).contains(1); step++) {
      await tester.dragFrom(const Offset(4, 400), Offset(0, 260 * away));
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 50));
    }
    final trail = <List<int>>[];
    for (var step = 0; step < 400 && !onScreen(tester).contains(120); step++) {
      await tester.dragFrom(const Offset(4, 400), Offset(0, -260 * away));
      await tester.pump();
      tick.value++;
      await tester.pump(const Duration(milliseconds: 50));
      tick.value++;
      await tester.pump(const Duration(milliseconds: 50));
      trail.add(onScreen(tester));
      expectInOrder(tester, 'back $step');
    }
    expect(onScreen(tester), contains(120), reason: 'never got back: ${trail.take(12).toList()} ... ${trail.reversed.take(6).toList()}');
  });
}
