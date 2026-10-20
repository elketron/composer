import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';

import '../../state/server_status.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../chat/navigation.dart';
import 'activity_dot.dart';
import 'approval_tile.dart';
import 'new_chat_sheet.dart';
import 'section_label.dart';
import 'session_card.dart';

/// A server's sessions by what they need: you (approvals, finished work you haven't seen),
/// running, and the rest, newest first.
class ChatsTab extends StatelessWidget {
  const ChatsTab({super.key, required this.url, required this.status});

  final String url;
  final ServerStatus status;

  @override
  Widget build(BuildContext context) {
    final sessions = status.sessions;
    final ids = {for (final s in sessions) s.id};
    // Workers show inside their orchestrator's card; on their own only when the orchestrator is gone.
    final top = sessions.where((s) => s.parent == null || !ids.contains(s.parent)).toList();
    bool running(SessionInfo s) =>
        isBusy(status.activity(s)) || sessions.any((w) => w.parent == s.id && isBusy(status.activity(w)));
    final unread = sessions.where((s) => status.unread.contains(s.id) && !running(s)).toList();
    final busy = top.where(running).toList();
    final rest = top.where((s) => !running(s) && !status.unread.contains(s.id)).toList();
    final needsYou = status.approvals.length + unread.length;

    Widget card(SessionInfo s) => Padding(
          padding: const EdgeInsets.only(bottom: 8),
          child: SessionCard(status: status, session: s, unread: status.unread.contains(s.id), onTap: () => openSession(context, url, s.id)),
        );

    return Stack(children: [
      ListView(
        key: const PageStorageKey('chats'),
        padding: const EdgeInsets.fromLTRB(14, 0, 14, 88),
        children: [
          if (needsYou > 0) ...[
            SectionLabel('Needs you', trailing: _Count(needsYou)),
            for (final a in status.approvals)
              Padding(
                padding: const EdgeInsets.only(bottom: 8),
                child: ApprovalTile(url: url, approval: a, agent: status.session(a.session)?.agent),
              ),
            for (final s in unread) card(s),
          ],
          if (busy.isNotEmpty) ...[const SectionLabel('Running'), for (final s in busy) card(s)],
          if (rest.isNotEmpty) ...[const SectionLabel('Recent'), for (final s in rest) card(s)],
          if (sessions.isEmpty)
            const Padding(
              padding: EdgeInsets.only(top: 48),
              child: Text('No chats yet. Start one with New chat.', textAlign: TextAlign.center, style: TextStyle(color: t.faint)),
            ),
        ],
      ),
      Positioned(
        right: 14,
        bottom: 14,
        child: FilledButton.icon(
          key: const Key('new-chat'),
          style: FilledButton.styleFrom(minimumSize: const Size(0, 46), padding: const EdgeInsets.symmetric(horizontal: 16), elevation: 6),
          onPressed: () => showNewChat(context, url),
          icon: const Icon(Icons.add, size: 20),
          label: const Text('New chat', style: TextStyle(fontSize: 14)),
        ),
      ),
    ]);
  }
}

class _Count extends StatelessWidget {
  const _Count(this.n);
  final int n;

  @override
  Widget build(BuildContext context) => Container(
        padding: const EdgeInsets.symmetric(horizontal: 5),
        decoration: BoxDecoration(border: Border.all(color: t.warn.withValues(alpha: 0.5)), borderRadius: BorderRadius.circular(t.radiusSm)),
        child: Text('$n', style: mono(size: 11, color: t.warn)),
      );
}
