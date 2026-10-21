import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../../state/server_status.dart';
import '../../state/types.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../chat/navigation.dart';
import '../home/activity_dot.dart';
import '../home/new_chat_sheet.dart';
import '../store_scope.dart';

/// The session list folded: one square per session (two letters of its agent and its state),
/// an orchestrator's workers under it by ticket, the running ones first.
class SessionStrip extends StatelessWidget {
  const SessionStrip({super.key, required this.url, required this.status});

  static const width = 44.0;

  final String url;
  final ServerStatus status;

  @override
  Widget build(BuildContext context) {
    final sessions = status.sessions;
    final ids = {for (final s in sessions) s.id};
    final top = sessions.where((s) => s.parent == null || !ids.contains(s.parent)).toList();
    bool running(SessionInfo s) =>
        isBusy(status.activity(s)) || sessions.any((w) => w.parent == s.id && isBusy(status.activity(w)));
    final ordered = [...top.where(running), ...top.where((s) => !running(s))];
    final current = status.chat?.session;

    Widget square(SessionInfo s, {bool worker = false}) {
      final on = s.id == current;
      final label = worker ? (s.ticket ?? s.id).replaceFirst(RegExp(r'^0+'), '') : s.agent.substring(0, s.agent.length.clamp(0, 2));
      return Tooltip(
        message: '${s.agent} #${s.id}${s.preview == null ? '' : ' · ${s.preview}'}',
        waitDuration: const Duration(milliseconds: 300),
        child: InkWell(
          key: Key('strip-${s.id}'),
          onTap: () => worker
              ? StoreScope.read(context).dispatch(SideTabSelected(url, SideTab.worker, session: s.id))
              : openSession(context, url, s.id),
          borderRadius: BorderRadius.circular(t.radius),
          child: Container(
            width: worker ? 24 : 30,
            height: worker ? 20 : 30,
            margin: EdgeInsets.only(left: worker ? 8 : 0, bottom: 4),
            decoration: BoxDecoration(
              color: on ? t.raised : null,
              border: Border.all(color: on ? t.borderStrong : Colors.transparent),
              borderRadius: BorderRadius.circular(t.radius),
            ),
            child: Stack(children: [
              Center(child: Text(label, style: mono(size: worker ? 9.5 : 11, color: on ? t.text : t.muted, weight: FontWeight.w600))),
              Positioned(right: 1, bottom: 1, child: ActivityDot(status.activity(s), size: 6)),
            ]),
          ),
        ),
      );
    }

    return Container(
      width: width,
      decoration: const BoxDecoration(border: Border(right: BorderSide(color: t.border))),
      child: Column(children: [
        IconButton(
          key: const Key('expand-sessions'),
          tooltip: 'Show the sessions (ctrl b)',
          onPressed: () => StoreScope.read(context).dispatch(const SessionsToggled()),
          icon: const Icon(Icons.chevron_right, size: 18, color: t.muted),
        ),
        const Divider(indent: 10, endIndent: 10),
        Expanded(
          child: ListView(padding: const EdgeInsets.symmetric(vertical: 4), children: [
            for (final s in ordered) ...[
              Center(child: square(s)),
              for (final w in sessions.where((w) => w.parent == s.id)) Center(child: square(w, worker: true)),
            ],
          ]),
        ),
        IconButton(
          tooltip: 'New chat',
          onPressed: () => showNewChat(context, url),
          icon: const Icon(Icons.add, size: 18, color: t.muted),
        ),
      ]),
    );
  }
}
