// The diff view: tap a line, write a comment, send; what reaches the effect runner is the
// ticket.comment command with the anchor and the quoted line. Real reducer and store, the
// effect runner only records.

import 'package:composer_app/flux/store.dart';
import 'package:composer_app/state/actions.dart';
import 'package:composer_app/state/app_state.dart';
import 'package:composer_app/state/effects.dart';
import 'package:composer_app/state/reduce.dart';
import 'package:composer_app/state/server_entry.dart';
import 'package:composer_app/theme/composer_theme.dart';
import 'package:composer_app/ui/chat/diff_page.dart';
import 'package:composer_app/ui/chat/review_view.dart';
import 'package:composer_app/ui/store_scope.dart';
import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

const home = ServerEntry(url: 'https://home.ts.net', name: 'home', token: 't1');

const unified = '''diff --git a/a.rs b/a.rs
--- a/a.rs
+++ b/a.rs
@@ -1,3 +1,3 @@
 fn a() {
-    let x = 1;
+    let x = 2;
 }
diff --git a/n.txt b/n.txt
new file
--- /dev/null
+++ b/n.txt
@@ -0,0 +1,1 @@
+plain
''';

const difft = 'a.rs --- Rust\n1    fn a() {\n2        let x = 1;\n   2     let x = 2;\n   3 }\n';

ProjectDiff diff({String? semantic}) => ProjectDiff(
      path: '/wt',
      files: [
        ChangedFile(path: 'a.rs', status: 'M', added: 1, removed: 1, semantic: semantic),
        const ChangedFile(path: 'n.txt', status: 'A', added: 1, removed: 0),
      ],
      diff: unified,
      ticket: const TicketReview(ticket: '0007', branch: 'ticket/0007', target: 'main', ahead: 1, commits: ['abc work'], uncommitted: false, conflicts: []),
    );

class Harness {
  final sent = <AppEffect>[];
  late final AppStore store;

  Harness(ProjectDiff changes) {
    store = Store(AppState.initial, reduce, (effect, dispatch) => sent.add(effect));
    store.dispatch(const ServersLoaded([home]));
    store.dispatch(const ReviewRequested('https://home.ts.net', '0007'));
    store.dispatch(TicketReplied(home.url, '0007', result: CommandResultChanges(changes: changes)));
    sent.clear();
  }

  Iterable<ClientCommand> get commands => sent.whereType<TicketCommand>().map((e) => e.command);

  Future<void> show(WidgetTester tester, Widget page) async {
    tester.view.physicalSize = const Size(400, 900);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.reset);
    await tester.pumpWidget(StoreScope(
      store: store,
      child: MaterialApp(theme: composerTheme(), home: page),
    ));
  }
}

void main() {
  testWidgets('review shows a View diff button instead of the diff, which opens the diff view', (tester) async {
    final h = Harness(diff());
    await h.show(tester, const Scaffold(body: ReviewView(url: 'https://home.ts.net', ticket: '0007')));
    expect(find.text('+    let x = 2;'), findsNothing);
    await tester.tap(find.byKey(const Key('view-diff')));
    await tester.pumpAndSettle();
    expect(find.text('+    let x = 2;'), findsOneWidget);
    expect(find.byKey(const Key('merge-ticket')), findsOneWidget, reason: 'Merge stays reachable from the diff view');
  });

  testWidgets('tap a line, comment, edit, send: the command carries the anchor and the quote', (tester) async {
    final h = Harness(diff());
    await h.show(tester, const DiffPage(url: 'https://home.ts.net', ticket: '0007'));
    expect(find.byKey(const Key('diff-mode')), findsNothing, reason: 'no semantic diff: text only');

    await tester.tap(find.text('+    let x = 2;'));
    await tester.pump();
    await tester.enterText(find.byKey(const Key('comment-field')), 'why 2?');
    await tester.tap(find.byKey(const Key('comment-add')));
    await tester.pump();
    expect(find.text('a.rs:2:2 — why 2?'), findsOneWidget);

    // A hunk is commentable too; edit that comment, then drop it.
    await tester.tap(find.text('@@ -0,0 +1,1 @@'));
    await tester.pump();
    await tester.enterText(find.byKey(const Key('comment-field')), 'new file?');
    await tester.tap(find.byKey(const Key('comment-add')));
    await tester.pump();
    await tester.tap(find.byKey(const Key('draft-edit-1')));
    await tester.pump();
    await tester.enterText(find.byKey(const Key('comment-field')), 'new file, why?');
    await tester.tap(find.byKey(const Key('comment-add')));
    await tester.pump();
    expect(find.text('n.txt:1:1 — new file, why?'), findsOneWidget);
    await tester.tap(find.byKey(const Key('draft-remove-1')));
    await tester.pump();
    expect(find.textContaining('n.txt:1:1'), findsNothing);

    await tester.tap(find.byKey(const Key('send-comments')));
    await tester.pump();
    final send = h.commands.single as ClientCommandTicketComment;
    expect(send.ticket, '0007');
    expect([for (final c in send.comments) (c.file, c.start, c.end, c.message, c.quote)], [('a.rs', 2, 2, 'why 2?', 'let x = 2;')]);

    // The server's answer: the page's comment lines, shown; the drafts are gone.
    h.store.dispatch(TicketReplied(home.url, '0007', result: const CommandResultComments(lines: ['- a.rs:2:2 — `let x = 2;`: why 2?'])));
    await tester.pump();
    expect(find.text('- a.rs:2:2 — `let x = 2;`: why 2?'), findsOneWidget);
    expect(find.byKey(const Key('send-comments')), findsNothing);
  });

  testWidgets('the semantic diff is shown when the server sent one, with the text diff a tap away', (tester) async {
    final h = Harness(diff(semantic: difft));
    await h.show(tester, const DiffPage(url: 'https://home.ts.net', ticket: '0007'));
    expect(find.byKey(const Key('diff-mode')), findsOneWidget);
    expect(find.text('   2     let x = 2;'), findsOneWidget);
    expect(find.text('+    let x = 2;'), findsNothing);
    expect(find.text('+plain'), findsOneWidget, reason: 'n.txt has no semantic diff: its text diff');

    await tester.tap(find.text('   2     let x = 2;'));
    await tester.pump();
    await tester.enterText(find.byKey(const Key('comment-field')), 'ok');
    await tester.tap(find.byKey(const Key('comment-add')));
    await tester.pump();
    await tester.tap(find.byKey(const Key('send-comments')));
    await tester.pump();
    final c = (h.commands.single as ClientCommandTicketComment).comments.single;
    expect((c.file, c.start, c.end, c.quote), ('a.rs', 2, 2, 'let x = 2;'));

    await tester.tap(find.text('Text'));
    await tester.pump();
    expect(find.text('+    let x = 2;'), findsOneWidget);
  });

  testWidgets('a long press marks a range, the next tap comments on the span; the anchor is start:end', (tester) async {
    final h = Harness(diff());
    await h.show(tester, const DiffPage(url: 'https://home.ts.net', ticket: '0007'));
    await tester.longPress(find.text(' fn a() {'));
    await tester.pump();
    await tester.tap(find.text('+    let x = 2;'));
    await tester.pump();
    expect(find.text('a.rs:1:2'), findsOneWidget);
    await tester.enterText(find.byKey(const Key('comment-field')), 'both');
    await tester.tap(find.byKey(const Key('comment-add')));
    await tester.pump();
    expect(find.text('a.rs:1:2 — both'), findsOneWidget);
    await tester.tap(find.byKey(const Key('send-comments')));
    await tester.pump();
    final c = (h.commands.single as ClientCommandTicketComment).comments.single;
    expect((c.file, c.start, c.end), ('a.rs', 1, 2));
  });

  testWidgets('editing a range draft keeps its end; the mark is spent after use; the whole range shows it has a comment', (tester) async {
    final h = Harness(diff());
    await h.show(tester, const DiffPage(url: 'https://home.ts.net', ticket: '0007'));
    final marked = find.byWidgetPredicate((w) => w is Container && w.color != null && w.color!.a > 0.1 && w.color!.a < 0.2);
    await tester.longPress(find.text(' fn a() {'));
    await tester.pump();
    expect(marked, findsOneWidget, reason: 'the marked start is highlighted');
    await tester.tap(find.text(' }'));
    await tester.pump();
    await tester.enterText(find.byKey(const Key('comment-field')), 'span');
    await tester.tap(find.byKey(const Key('comment-add')));
    await tester.pump();
    expect(find.text('a.rs:1:3 — span'), findsOneWidget);
    expect(marked, findsNothing, reason: 'the highlight goes with the mark');
    // hunk, context, removed, added, context: every line of the range, not only its ends.
    expect(find.byIcon(Icons.comment), findsNWidgets(5));

    await tester.tap(find.byKey(const Key('draft-edit-0')));
    await tester.pump();
    await tester.enterText(find.byKey(const Key('comment-field')), 'span, better');
    await tester.tap(find.byKey(const Key('comment-add')));
    await tester.pump();
    expect(find.text('a.rs:1:3 — span, better'), findsOneWidget);

    // The mark was spent by the first comment: a plain tap is one line again.
    await tester.tap(find.text('+    let x = 2;'));
    await tester.pump();
    expect(find.text('a.rs:2:2'), findsOneWidget);
    await tester.enterText(find.byKey(const Key('comment-field')), 'one');
    await tester.tap(find.byKey(const Key('comment-add')));
    await tester.pump();
    await tester.tap(find.byKey(const Key('send-comments')));
    await tester.pump();
    final sent = (h.commands.single as ClientCommandTicketComment).comments;
    expect([for (final c in sent) (c.start, c.end, c.message)], [(1, 3, 'span, better'), (2, 2, 'one')]);
  });
}
