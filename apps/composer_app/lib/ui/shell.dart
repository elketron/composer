import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'package:composer_protocol/composer_protocol.dart';

import '../state/actions.dart';
import '../state/approval_text.dart';
import 'app_shortcuts.dart';
import 'chat/navigation.dart';
import 'chat/review_page.dart';
import 'home/new_chat_sheet.dart';
import 'home/phone_home.dart';
import 'pages/page_navigation.dart';
import 'store_scope.dart';
import 'wide/command_palette.dart';
import 'wide/wide_layout.dart';
import 'welcome_page.dart';

/// The app frame: `WideLayout` on wide screens (ctrl-b folds the sessions, esc closes the side
/// pane); on phones `PhoneHome` (tabs,
/// the title opens the switcher). ctrl-1…9 switches servers.
class Shell extends StatelessWidget {
  const Shell({super.key});

  static const wide = 720.0;

  @override
  Widget build(BuildContext context) {
    final store = StoreScope.of(context);
    final state = store.state;
    if (!state.loaded) return const Scaffold();
    final server = state.activeServer;
    if (server == null) return const WelcomePage();
    final status = state.statusOf(server.url);

    return AppShortcuts(
      bindings: {
        for (var i = 0; i < state.servers.length && i < 9; i++)
          SingleActivator(LogicalKeyboardKey(LogicalKeyboardKey.digit1.keyId + i), control: true):
              () => store.dispatch(ServerSelected(state.servers[i].url)),
        const SingleActivator(LogicalKeyboardKey.keyB, control: true): () => store.dispatch(const SessionsToggled()),
        const SingleActivator(LogicalKeyboardKey.keyK, control: true): () => showPalette(context, server.url),
        const SingleActivator(LogicalKeyboardKey.keyN, control: true): () => showNewChat(context, server.url),
        const SingleActivator(LogicalKeyboardKey.keyP, control: true): () =>
            openPage(context, server.url, status.page.page?.url ?? 'site://home'),
        const SingleActivator(LogicalKeyboardKey.keyD, control: true, shift: true): () => _changes(context, store, server.url),
        const SingleActivator(LogicalKeyboardKey.arrowUp, alt: true): () => _step(context, store, server.url, -1),
        const SingleActivator(LogicalKeyboardKey.arrowDown, alt: true): () => _step(context, store, server.url, 1),
        const SingleActivator(LogicalKeyboardKey.bracketRight, control: true): () => _nextWorker(context, store, server.url),
        // The open chat's approval (the oldest), as in the TUI.
        if (status.approvals.any((a) => a.session == status.chat?.session)) ...{
          const SingleActivator(LogicalKeyboardKey.keyY): () => _answer(store, server.url, ApprovalDecision.approve),
          const SingleActivator(LogicalKeyboardKey.keyN): () => _answer(store, server.url, ApprovalDecision.deny),
          const SingleActivator(LogicalKeyboardKey.keyA): () => _answer(store, server.url, ApprovalDecision.always),
        },
        if (status.side != null) const SingleActivator(LogicalKeyboardKey.escape): () => store.dispatch(SideClosed(server.url)),
      },
      child: Focus(
        autofocus: true,
        child: LayoutBuilder(builder: (context, constraints) {
          if (constraints.maxWidth >= wide) {
            return WideLayout(key: ValueKey(server.url), server: server, status: status, width: constraints.maxWidth);
          }
          return PhoneHome(key: ValueKey(server.url), server: server, status: status);
        }),
      ),
    );
  }

  /// ctrl-shift-d: the open worker's changes.
  void _changes(BuildContext context, AppStore store, String url) {
    final status = store.state.statusOf(url);
    final ticket = status.chat == null ? null : status.session(status.chat!.session)?.ticket;
    if (ticket == null) {
      store.dispatch(const NoticeShown('Changes are a worker\'s: open one (or its Changes on the orchestrator\'s card).'));
      return;
    }
    openReview(context, url, ticket);
  }

  /// alt-↑ / alt-↓: the previous or next chat in the list.
  void _step(BuildContext context, AppStore store, String url, int by) {
    final status = store.state.statusOf(url);
    final ids = [for (final s in status.sessions) if (s.parent == null) s.id];
    if (ids.isEmpty) return;
    final at = ids.indexOf(status.chat?.session ?? '');
    final next = at < 0 ? 0 : (at + by).clamp(0, ids.length - 1);
    openSession(context, url, ids[next]);
  }

  /// ctrl-]: the next session of this orchestration, as the TUI's ]w.
  void _nextWorker(BuildContext context, AppStore store, String url) {
    final status = store.state.statusOf(url);
    final current = status.chat?.session;
    if (current == null) return;
    final members = [for (final s in status.orchestration(current)) s.id];
    if (members.length < 2) return;
    openSession(context, url, members[(members.indexOf(current) + 1) % members.length]);
  }

  void _answer(AppStore store, String url, ApprovalDecision decision) {
    final status = store.state.statusOf(url);
    final approval = status.approvals.where((a) => a.session == status.chat?.session).firstOrNull;
    if (approval == null) return;
    // `always` only where it means something.
    if (decision == ApprovalDecision.always && !approvalCanAlways(approval)) return;
    store.dispatch(ApprovalAnswered(url, approval.request, decision));
  }
}
