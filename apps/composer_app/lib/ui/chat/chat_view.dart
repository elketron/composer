import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';

import '../../state/chat_state.dart';
import '../../state/server_status.dart';
import '../../theme/tokens.dart' as t;
import '../pages/page_navigation.dart';
import '../../state/chat_items.dart';
import '../../state/chat_rows.dart';
import 'activity_line.dart';
import 'agent_text.dart';
import 'approval_card.dart';
import 'chat_header.dart';
import 'chat_menu.dart';
import 'harness_chip.dart';
import 'project_chip.dart';
import 'chat_item_view.dart';
import 'steps_view.dart';
import 'worker_card.dart';
import 'message_composer.dart';
import 'older_messages.dart';
import 'orchestration_bar.dart';

/// A session's chat: transcript (newest at the bottom, following the stream), its approvals,
/// and the composer.
class ChatView extends StatelessWidget {
  const ChatView({super.key, required this.url, required this.status, required this.chat, this.onClose, this.header = true, this.compact = false});

  final String url;
  final ServerStatus status;
  final ChatState chat;
  final VoidCallback? onClose;

  /// Phones put the session in the app bar instead.
  final bool header;

  /// Beside another chat (the side pane's Worker tab): no orchestration strip and no composer,
  /// which writes to the open chat.
  final bool compact;

  @override
  Widget build(BuildContext context) {
    final session = status.session(chat.session);
    final parent = session?.parent == null ? null : status.session(session!.parent!);
    final busy = session != null && session.state != TurnState.idle;
    final streaming = chat.streaming;
    final approvals = status.approvals.where((a) => a.session == chat.session);

    // Built bottom-up: the list is reversed so it sticks to the newest line. Keys keep each
    // row's state (an expanded tool row) on its row as new ones arrive below.
    // A turn's steps fold into one row; workers' reports become cards.
    final chatRowsList = chatRows(chat.items);
    final rows = <(Key, Widget)>[
      for (final approval in approvals.toList().reversed)
        (ValueKey('approval-${approval.request}'), ApprovalCard(url: url, approval: approval)),
      if (busy) (const ValueKey('activity'), ActivityLine(chat: chat, state: session.state)),
      if (streaming != null && streaming.content.isNotEmpty)
        (const ValueKey('streaming'), AgentText(streaming.content, url: url, streaming: true)),
      for (final (i, row) in chatRowsList.indexed.toList().reversed)
        (
          ValueKey('row-${i - chat.rowBase}'),
          switch (row) {
            ItemRow(:final item) => ChatItemView(item, url: url),
            StepsRow() => StepsView(row: row, url: url),
            WorkerRow() => WorkerCard(row: row, url: url),
          },
        ),
      if (chat.more) (const ValueKey('older'), OlderMessages(url: url, session: chat.session, loading: chat.loadingOlder)),
    ];
    final keys = {for (final (i, (key, _)) in rows.indexed) key: i};

    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      if (header)
        ChatHeader(
          sessionId: chat.session,
          session: session,
          parent: parent,
          onClose: onClose,
          onBrowse: () => openPage(context, url, 'site://home'),
          menu: ChatMenu(url: url, session: session),
          // A worker's project is its worktree: no chips to change it.
          chips: session == null || session.ticket != null
              ? null
              : [
                  ProjectChip(url: url, project: session.project),
                  if (session.project != null) HarnessChip(url: url, project: session.project!),
                ],
        ),
      if (!compact) OrchestrationBar(url: url, status: status, session: chat.session),
      Expanded(
        child: rows.isEmpty
            ? Center(child: Text(chat.loaded && chat.lastSeq == 0 ? 'Say something to start.' : '', style: const TextStyle(color: t.faint)))
            : SelectionArea(
                child: ListView.builder(
                  key: PageStorageKey('chat-$url-${chat.session}${compact ? '-side' : ''}'),
                  reverse: true,
                  padding: const EdgeInsets.fromLTRB(16, 4, 16, 14),
                  itemCount: rows.length,
                  findChildIndexCallback: (key) => keys[key],
                  itemBuilder: (_, i) => Padding(
                    key: rows[i].$1,
                    padding: const EdgeInsets.only(top: 10),
                    child: Center(
                      child: ConstrainedBox(
                        constraints: const BoxConstraints(maxWidth: 820),
                        child: SizedBox(width: double.infinity, child: rows[i].$2),
                      ),
                    ),
                  ),
                ),
              ),
      ),
      if (!compact) MessageComposer(key: ValueKey('composer-$url-${chat.session}'), url: url, busy: busy),
    ]);
  }
}
