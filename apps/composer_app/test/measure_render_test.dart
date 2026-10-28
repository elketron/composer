// Throwaway measurement (ticket 0053): the render half of opening a chat — state set -> first
// frame — with the real widgets (SelectionArea + MarkdownBody selectable), on a phone-sized
// surface. Prints wall-clock medians; not a pass/fail test.

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

String para(int n) => '''
message number $n, a paragraph of markdown with some **bold** text and a [link](site://memory/user),
plus a `code span` and a list:

- first point about the transcript
- second point, a little longer so the row grows

```
cargo test -p composer-server
```

and a final line.''';

ServerMessage said(int seq, String text) => ServerMessageMessageDone(
      session: 'a',
      seq: seq,
      message: ChatMessage(role: Role.user, content: text),
    );
ServerMessage replied(int seq, String text) => ServerMessageMessageDone(
      session: 'a',
      seq: seq,
      message: ChatMessage(role: Role.assistant, content: text, thinking: seq.isEven ? 'a thought, long enough to fold into a row of its own, with **markdown**' : null),
    );

void main() {
  late AppStore store;

  Future<double> openChat(WidgetTester tester, int messages) async {
    await tester.binding.setSurfaceSize(const Size(400, 800));
    store = Store(AppState.initial, reduce, (effect, dispatch) {});
    store.dispatch(const ServersLoaded([home], StorageKind.file));
    store.dispatch(SessionOpened(home.url, 'a'));
    await tester.pumpWidget(
      StoreScope(
        store: store,
        child: MaterialApp(
          home: Scaffold(
            body: Builder(
              builder: (context) {
                final status = StoreScope.of(context).state.statusOf(home.url);
                return ChatView(url: home.url, status: status, chat: status.chat!, header: false, compact: true);
              },
            ),
          ),
        ),
      ),
    );
    await tester.pump();
    final events = <ServerMessage>[
      for (var i = 1; i <= messages; i++) ...[
        said(i * 2 - 1, 'user message $i, what the agent should look at next, with some length to it'),
        replied(i * 2, para(i)),
      ],
    ];
    final watch = Stopwatch()..start();
    store.dispatch(HistoryLoaded(home.url, 'a', null, events));
    await tester.pump();
    watch.stop(); // state set (reduce + rebuild scheduled)
    final frame = Stopwatch()..start();
    await tester.pump();
    frame.stop(); // first frame after the state was set
    print('render ${messages}msgs: state ${watch.elapsedMilliseconds}ms frame ${frame.elapsedMilliseconds}ms (wall)');
    return frame.elapsedMicroseconds / 1000.0;
  }

  testWidgets('render cost', (tester) async {
    for (var run = 0; run < 3; run++) {
      await openChat(tester, 6);
    }
    for (var run = 0; run < 3; run++) {
      await openChat(tester, 50);
    }
  });
}
