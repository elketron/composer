import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';

import '../../state/inbox.dart';
import '../../state/server_status.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../chat/navigation.dart';
import '../pipelines/gate_tile.dart';
import 'approval_tile.dart';
import 'new_chat_sheet.dart';
import 'section_label.dart';
import 'session_card.dart';

/// The Inbox: a server's sessions by what they need: you (approvals, finished work you haven't seen),
/// running, and the rest, newest first.
class InboxTab extends StatelessWidget {
  const InboxTab({super.key, required this.url, required this.status});

  final String url;
  final ServerStatus status;

  @override
  Widget build(BuildContext context) {
    final sessions = status.sessions;
    final inbox = Inbox.of(status);
    final (approvals, gates, unread, busy, rest, needsYou) = (inbox.approvals, inbox.gates, inbox.unread, inbox.busy, inbox.rest, inbox.needsYou);

    Widget card(SessionInfo s) => Padding(
      padding: const EdgeInsets.only(bottom: 8),
      child: SessionCard(
        status: status,
        session: s,
        unread: status.unread.contains(s.id),
        onTap: () => openSession(context, url, s.id),
        // A worker opens its own chat.
        onWorker: (w) => openSession(context, url, w.id),
      ),
    );

    return Stack(
      children: [
        ListView(
          key: const PageStorageKey('inbox'),
          padding: const EdgeInsets.fromLTRB(14, 0, 14, 88),
          children: [
            if (needsYou > 0) ...[
              SectionLabel('Needs you', trailing: _Count(needsYou)),
              for (final a in approvals)
                Padding(
                  padding: const EdgeInsets.only(bottom: 8),
                  child: ApprovalTile(
                    url: url,
                    approval: a,
                    agent: status.session(a.session)?.agent,
                  ),
                ),
              for (final w in gates)
                GateTile(url: url, status: status, work: w),
              for (final s in unread) card(s),
            ],
            if (busy.isNotEmpty) ...[
              const SectionLabel('Running'),
              for (final s in busy) card(s),
            ],
            if (rest.isNotEmpty) ...[
              const SectionLabel('Recent'),
              for (final s in rest) card(s),
            ],
            if (sessions.isEmpty)
              const Padding(
                padding: EdgeInsets.only(top: 48),
                child: Text(
                  'No chats yet. Start one with New chat.',
                  textAlign: TextAlign.center,
                  style: TextStyle(color: t.faint),
                ),
              ),
          ],
        ),
        Positioned(
          right: 14,
          bottom: 14,
          child: FilledButton.icon(
            key: const Key('new-chat'),
            style: FilledButton.styleFrom(
              minimumSize: const Size(0, 46),
              padding: const EdgeInsets.symmetric(horizontal: 16),
              elevation: 6,
            ),
            onPressed: () => showNewChat(context, url),
            icon: const Icon(Icons.add, size: 20),
            label: const Text('New chat', style: TextStyle(fontSize: 14)),
          ),
        ),
      ],
    );
  }
}

class _Count extends StatelessWidget {
  const _Count(this.n);
  final int n;

  @override
  Widget build(BuildContext context) => Container(
    padding: const EdgeInsets.symmetric(horizontal: 5),
    decoration: BoxDecoration(
      border: Border.all(color: t.warn.withValues(alpha: 0.5)),
      borderRadius: BorderRadius.circular(t.radiusSm),
    ),
    child: Text('$n', style: mono(size: 11, color: t.warn)),
  );
}
