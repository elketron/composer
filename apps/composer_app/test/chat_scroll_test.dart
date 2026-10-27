// The chat list follows the stream while you are at the bottom and holds its place when you
// scroll up. Real reducer, real widgets, no server.

import 'package:composer_app/flux/store.dart';
import 'package:composer_app/state/actions.dart';
import 'package:composer_app/state/app_state.dart';
import 'package:composer_app/state/reduce.dart';
import 'package:composer_app/state/server_entry.dart';
import 'package:composer_app/state/types.dart';
import 'package:composer_app/ui/chat/chat_view.dart';
import 'package:composer_app/ui/store_scope.dart';
import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

const home = ServerEntry(url: 'https://home.ts.net', name: 'home', token: 't1');

ServerMessage said(int seq, String text) => ServerMessageMessageDone(
  session: 'a',
  seq: seq,
  message: ChatMessage(role: Role.user, content: text),
);
ServerMessage delta(int seq, String text) =>
    ServerMessageMessageDelta(session: 'a', seq: seq, text: text);
const thoughtText = 'a thought, long enough to fold into a row of its own';

ServerMessage thought(int seq, String text) =>
    ServerMessageThinkingDelta(session: 'a', seq: seq, text: text);
ServerMessage replied(int seq, String text, {String? thinking}) =>
    ServerMessageMessageDone(
      session: 'a',
      seq: seq,
      message: ChatMessage(
        role: Role.assistant,
        content: text,
        thinking: thinking,
      ),
    );
ServerMessage turn(String state, int seq) => ServerMessageSessionUpdated(
  session: SessionInfo(
    id: 'a',
    agent: 'default',
    state: TurnState.values.byName(state),
    lastSeq: seq,
    updated: '2026-10-27',
  ),
);

const stream1 = '''
the reply, streaming:

line a1
line a2
line a3
line a4
line a5
line a6''';
const stream2 = '''
line a7
line a8
line a9
line a10
line a11
line a12''';
const reply = '''
the reply, streaming:

line a1
line a2
line a3
line a4
line a5
line a6

line a7
line a8
line a9
line a10
line a11
line a12''';

void main() {
  late AppStore store;

  Future<void> openChat(WidgetTester tester) async {
    await tester.binding.setSurfaceSize(const Size(400, 800));
    store = Store(AppState.initial, reduce, (effect, dispatch) {});
    store.dispatch(const ServersLoaded([home], StorageKind.file));
    store.dispatch(SessionOpened(home.url, 'a'));
    await tester.pumpWidget(
      StoreScope(
        store: store,
        child: MaterialApp(
          // On a real route the view sits in a Scaffold; its rows' InkWells need the Material.
          home: Scaffold(
            body: Builder(
              builder: (context) {
                final status = StoreScope.of(context).state.statusOf(home.url);
                final chat = status.chat!;
                return ChatView(
                  url: home.url,
                  status: status,
                  chat: chat,
                  header: false,
                  compact: true,
                );
              },
            ),
          ),
        ),
      ),
    );
    await tester.pump();
    store.dispatch(
      HistoryLoaded(home.url, 'a', null, [
        for (var i = 1; i <= 12; i++)
          said(
            i,
            'older message number one two three four five six, a few lines to make the transcript tall',
          ),
      ]),
    );
    await tester.pump();
  }

  Future<void> frame(WidgetTester tester) =>
      tester.pump(const Duration(milliseconds: 100));

  double y(WidgetTester tester, Key key) =>
      tester.getTopLeft(find.byKey(key)).dy;

  testWidgets('scrolled up during a stream, it holds its place', (
    tester,
  ) async {
    await openChat(tester);
    store.dispatch(ServerEvent(home.url, turn('running', 13)));
    store.dispatch(ServerEvent(home.url, delta(14, stream1)));
    await frame(tester);

    await tester.drag(find.byType(ListView), const Offset(0, 150));
    await frame(tester);
    final held = y(tester, const ValueKey('row-8'));
    expect(held, greaterThan(0), reason: 'the marker is on screen to be held');

    store.dispatch(ServerEvent(home.url, delta(15, stream2)));
    await frame(tester);
    await frame(tester);
    expect(
      y(tester, const ValueKey('row-8')),
      closeTo(held, 2),
      reason: 'a chunk lands below: what you read stays put',
    );

    store.dispatch(ServerEvent(home.url, replied(16, reply)));
    await frame(tester);
    await frame(tester);
    expect(
      y(tester, const ValueKey('row-8')),
      closeTo(held, 2),
      reason: 'and the message landing does not stick it somewhere else',
    );
  });

  testWidgets('at the bottom it keeps following', (tester) async {
    await openChat(tester);
    store.dispatch(ServerEvent(home.url, turn('running', 13)));
    store.dispatch(ServerEvent(home.url, delta(14, stream1)));
    await frame(tester);
    expect(
      find.textContaining('line a6'),
      findsWidgets,
      reason: 'the newest line is on screen',
    );
    final before = y(tester, const ValueKey('row-11'));

    store.dispatch(ServerEvent(home.url, delta(15, stream2)));
    await frame(tester);
    expect(
      y(tester, const ValueKey('row-11')),
      lessThan(before),
      reason: 'the stream grows: the view slides up with it',
    );
    expect(find.textContaining('line a12'), findsWidgets);

    store.dispatch(ServerEvent(home.url, replied(16, reply)));
    await frame(tester);
    expect(
      find.textContaining('line a12'),
      findsWidgets,
      reason: 'still at the newest line when the message lands',
    );
  });

  testWidgets('scrolled up, what lands is offered back with a tap', (
    tester,
  ) async {
    await openChat(tester);
    store.dispatch(ServerEvent(home.url, turn('running', 13)));
    store.dispatch(ServerEvent(home.url, delta(14, stream1)));
    await frame(tester);
    await tester.drag(find.byType(ListView), const Offset(0, 300));
    await frame(tester);
    expect(
      find.byKey(const Key('new-content')),
      findsNothing,
      reason: 'nothing new yet',
    );

    store.dispatch(ServerEvent(home.url, delta(15, stream2)));
    await frame(tester);
    expect(
      find.byKey(const Key('new-content')),
      findsNothing,
      reason: 'the chunk grew a row that was already there: nothing new to offer',
    );

    store.dispatch(ServerEvent(home.url, replied(16, reply)));
    await frame(tester);
    expect(find.byKey(const Key('new-content')), findsOneWidget);

    await tester.tap(find.byKey(const Key('new-content')));
    await frame(tester);
    await frame(tester);
    expect(
      find.textContaining('line a12'),
      findsWidgets,
      reason: 'the tap takes you back to the newest line',
    );
    expect(find.byKey(const Key('new-content')), findsNothing);
  });

  testWidgets(
    'the streamed row becomes the finished message, it is not rebuilt',
    (tester) async {
      await openChat(tester);
      store.dispatch(ServerEvent(home.url, turn('running', 13)));
      store.dispatch(ServerEvent(home.url, delta(14, stream1)));
      await frame(tester);
      final row = find.byKey(const ValueKey('row-12'));
      expect(
        row,
        findsOneWidget,
        reason: 'the streaming row carries the key the message will land in',
      );
      final element = tester.element(row);

      store.dispatch(ServerEvent(home.url, replied(15, reply)));
      await frame(tester);
      expect(
        tester.element(row),
        same(element),
        reason: 'same key: the same element',
      );
    },
  );

  testWidgets('a thinking block does not steal the finished message\'s row', (
    tester,
  ) async {
    await openChat(tester);
    store.dispatch(ServerEvent(home.url, turn('running', 13)));
    store.dispatch(ServerEvent(home.url, delta(14, stream1)));
    await frame(tester);
    store.dispatch(ServerEvent(home.url, thought(15, thoughtText)));
    await frame(tester);
    // With the thinking in, the streaming row already carries the key the finished message lands in.
    final row = find.byKey(const ValueKey('row-13'));
    expect(row, findsOneWidget);
    final element = tester.element(row);

    store.dispatch(
      ServerEvent(home.url, replied(16, reply, thinking: thoughtText)),
    );
    await frame(tester);
    expect(
      tester.element(row),
      same(element),
      reason: 'the message lands in the row it was already in',
    );
  });
}
