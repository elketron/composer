// Tapping a system notification lands on the chat it came from. Real app, real reducer, real
// Notifier (the test feeds it the response the plugin would deliver); only the effect runner is
// faked: it records commands and answers FetchHistory. Phone-sized, so chats open as pages.
//
// What is red before the fix: a tap while another chat is open (the pop the tap itself does
// counts as closing a chat) and a tap that started the app (nothing was listening yet).

import 'dart:convert';

import 'package:composer_app/flux/store.dart';
import 'package:composer_app/servers/notifier.dart';
import 'package:composer_app/state/actions.dart';
import 'package:composer_app/state/app_state.dart';
import 'package:composer_app/state/effects.dart';
import 'package:composer_app/state/reduce.dart';
import 'package:composer_app/state/server_entry.dart';
import 'package:composer_app/state/types.dart';
import 'package:composer_app/ui/composer_app.dart';
import 'package:composer_app/ui/store_scope.dart';
import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';
import 'package:flutter_local_notifications/flutter_local_notifications.dart';
import 'package:flutter_test/flutter_test.dart';

const home = ServerEntry(url: 'https://home.ts.net', name: 'home', token: 't1');

SessionInfo session(String id) =>
    SessionInfo(id: id, agent: 'default', state: TurnState.idle, lastSeq: 1, updated: '2026-10-27');

class Harness {
  late final AppStore store;
  final notifier = Notifier();
  final sent = <AppEffect>[];

  Harness() {
    store = Store(AppState.initial, reduce, (effect, dispatch) {
      sent.add(effect);
      if (effect is FetchHistory) {
        dispatch(HistoryLoaded(home.url, effect.session, null, [
          ServerMessageMessageDone(
            session: effect.session,
            seq: 1,
            message: ChatMessage(role: Role.assistant, content: 'the ${effect.session} transcript'),
          ),
        ]));
      }
    });
    store.dispatch(const ServersLoaded([home], StorageKind.file));
    store.dispatch(ServerEvent(home.url, ServerMessageOk(id: 1, result: CommandResultSessions(sessions: [session('a'), session('b')]))));
  }

  /// What the plugin hands the app when a notification (or one of its buttons) is tapped.
  void tap({String? session, String? approval, String? action}) {
    notifier.handleResponse(NotificationResponse(
      notificationResponseType:
          action == null ? NotificationResponseType.selectedNotification : NotificationResponseType.selectedNotificationAction,
      actionId: action,
      payload: jsonEncode({'url': home.url, 'session': session, 'approval': approval, 'page': null, 'app': null}),
    ));
  }

  Iterable<ClientCommand> get commands => sent.whereType<SendCommand>().map((e) => e.command);
  List<String> get attached => commands.whereType<ClientCommandSessionAttach>().map((c) => c.session).toList();
  List<String> get detached => commands.whereType<ClientCommandSessionDetach>().map((c) => c.session).toList();
  String get open => store.state.statusOf(home.url).chat?.session ?? '(none)';

  Future<void> boot(WidgetTester tester) async {
    // A phone, the view itself: MediaQuery still counts the 800x600 test window as wide.
    tester.view.physicalSize = const Size(400, 800);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.reset);
    await tester.binding.setSurfaceSize(const Size(400, 800));
    await tester.pumpWidget(ComposerApp(store: store, notifier: notifier));
    await settle(tester);
  }
}

Future<void> settle(WidgetTester tester) async {
  for (var i = 0; i < 6; i++) {
    await tester.pump(const Duration(milliseconds: 100));
  }
}

void main() {
  testWidgets('a tap while another chat is open lands on the chat it came from', (tester) async {
    final h = Harness();
    await h.boot(tester);
    await tester.tap(find.byKey(const Key('session-a')));
    await settle(tester);
    expect(find.textContaining('the a transcript'), findsOneWidget, reason: 'standing on chat a');

    h.tap(session: 'b');
    await settle(tester);

    expect(h.open, 'b');
    expect(find.textContaining('the b transcript'), findsOneWidget);
    expect(h.attached, contains('b'), reason: 'the chat you landed on is attached');
    expect(h.detached, isNot(contains('b')), reason: 'the pop the tap itself does is not closing a chat');
  });

  testWidgets('a tap while on the chats list (no chat open) lands on its chat', (tester) async {
    final h = Harness();
    await h.boot(tester);

    h.tap(session: 'b');
    await settle(tester);

    expect(h.open, 'b');
    expect(find.textContaining('the b transcript'), findsOneWidget);
    expect(h.attached, contains('b'));
    expect(h.detached, isEmpty);
  });

  testWidgets('a tap that started the app still lands on its chat', (tester) async {
    final h = Harness();
    h.tap(session: 'b');
    await h.boot(tester);

    expect(h.open, 'b');
    expect(find.textContaining('the b transcript'), findsOneWidget);
    expect(h.attached, contains('b'));
    expect(h.detached, isEmpty);
  });

  testWidgets('backing out of a chat the normal way still closes it', (tester) async {
    final h = Harness();
    await h.boot(tester);
    await tester.tap(find.byKey(const Key('session-a')));
    await settle(tester);
    expect(find.textContaining('the a transcript'), findsOneWidget);

    await tester.pageBack();
    await settle(tester);

    expect(h.open, '(none)');
    expect(h.detached, contains('a'));
  });

  testWidgets('Approve on a notification still answers it', (tester) async {
    final h = Harness();
    await h.boot(tester);

    h.tap(session: 'a', approval: 'r1', action: 'approve');
    await settle(tester);

    expect(h.commands.whereType<ClientCommandApprovalRespond>().map((c) => c.decision), [ApprovalDecision.approve]);
    expect(h.open, '(none)', reason: 'answering does not open a chat');
  });
}
