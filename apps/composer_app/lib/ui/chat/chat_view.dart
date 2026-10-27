import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';

import '../../state/actions.dart';
import '../../state/chat_state.dart';
import '../../state/server_status.dart';
import '../../state/types.dart';
import '../../theme/tokens.dart' as t;
import '../pages/page_navigation.dart';
import '../../state/chat_items.dart';
import '../../state/chat_rows.dart';
import '../store_scope.dart';
import 'activity_line.dart';
import 'agent_text.dart';
import 'approval_card.dart';
import 'chat_header.dart';
import 'chat_menu.dart';
import 'harness_chip.dart';
import 'project_chip.dart';
import 'chat_item_view.dart';
import 'new_content_pill.dart';
import 'steps_view.dart';
import 'worker_card.dart';
import 'message_composer.dart';
import 'older_messages.dart';
import 'orchestration_bar.dart';

/// A session's chat: transcript (newest at the bottom, following the stream), its approvals,
/// and the composer.
class ChatView extends StatefulWidget {
  const ChatView({super.key, required this.url, required this.status, required this.chat, this.onClose, this.header = true, this.compact = false, this.pane = ChatPane.main});

  final String url;
  final ServerStatus status;
  final ChatState chat;
  final VoidCallback? onClose;

  /// Phones put the session in the app bar instead.
  final bool header;

  /// Beside another chat (the side pane's Worker tab): no orchestration strip and no composer,
  /// which writes to the open chat.
  final bool compact;

  /// Which view this is: a session shown in both keeps a scroll position per one.
  final ChatPane pane;

  @override
  State<ChatView> createState() => _ChatViewState();
}

class _ChatViewState extends State<ChatView> {
  /// The list is reversed, so the newest line is where the scroll starts.
  final _controller = ScrollController();

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  bool _atBottom(ScrollMetrics m) => m.pixels <= m.minScrollExtent + 4;

  /// Where each row sits in the list, by key. The list is fed bottom-up, so a row growing down
  /// there moves everything above it; the rows still in place afterwards say by how much.
  Map<Key, double> _rows() {
    final found = <Key, double>{};
    void walk(Element e) {
      final d = e.renderObject?.parentData;
      if (e.widget.key != null && d is SliverLogicalParentData && e.renderObject is RenderBox) found[e.widget.key!] = d.layoutOffset!;
      e.visitChildren(walk);
    }

    walk(context as Element);
    return found;
  }

  /// The view follows the stream only while it was at the bottom: otherwise the rows above give
  /// back whatever a row growing down there pushed them by. [before] and [pixels] are from before
  /// this frame laid out; the topmost row still in place is the reference.
  void _hold(Map<Key, double> before, double pixels) {
    if (!mounted || !_controller.hasClients) return;
    final pos = _controller.position;
    // The view itself moved in between: that was the user, and they keep where they dragged it.
    if ((pos.pixels - pixels).abs() > 0.5) return;
    final after = _rows();
    final key = before.keys.toList().reversed.where(after.containsKey).firstOrNull;
    if (key == null) return;
    final moved = after[key]! - before[key]!;
    if (moved != 0) pos.jumpTo((pos.pixels + moved).clamp(pos.minScrollExtent, pos.maxScrollExtent));
  }

  bool _onScroll(ScrollNotification n) {
    final pixels = _controller.hasClients ? _controller.position.pixels : 0.0;
    final atBottom = _atBottom(n.metrics);
    if (atBottom != widget.chat.atBottom) {
      StoreScope.read(context).dispatch(ViewScrolled(widget.url, widget.chat.session, atBottom: atBottom, pane: widget.pane));
    }
    if (!widget.chat.atBottom) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (!mounted) return;
        _hold(_rows(), pixels);
      });
    }
    return false;
  }

  void _backToLatest() {
    final pos = _controller.hasClients ? _controller.position : null;
    if (pos != null) pos.jumpTo(pos.minScrollExtent);
    StoreScope.read(context).dispatch(ViewScrolled(widget.url, widget.chat.session, atBottom: true, pane: widget.pane));
  }

  @override
  Widget build(BuildContext context) {
    final chat = widget.chat;
    final session = widget.status.session(chat.session);
    final parent = session?.parent == null ? null : widget.status.session(session!.parent!);
    final busy = session != null && session.state != TurnState.idle;
    final streaming = chat.streaming;
    final approvals = widget.status.approvals.where((a) => a.session == chat.session);

    // What the list will have laid out is only known at the end of this frame; that is when a row
    // that grew below gives its place back.
    if (!chat.atBottom) {
      final before = _rows();
      final pixels = _controller.hasClients ? _controller.position.pixels : 0.0;
      if (before.isNotEmpty) WidgetsBinding.instance.addPostFrameCallback((_) => _hold(before, pixels));
    }

    // Built bottom-up: the list is reversed so it sticks to the newest line. Keys keep each
    // row's state (an expanded tool row) on its row as new ones arrive below.
    // A turn's steps fold into one row; workers' reports become cards.
    final chatRowsList = chatRows(chat.items);
    final rows = <(Key, Widget)>[
      for (final approval in approvals.toList().reversed)
        (ValueKey('approval-${approval.request}'), ApprovalCard(url: widget.url, approval: approval)),
      if (busy) (const ValueKey('activity'), ActivityLine(chat: chat, state: session.state)),
      if (streaming != null && streaming.content.isNotEmpty)
        // The key the message lands in when it finishes: the row is kept, not rebuilt.
        (ValueKey('row-${pendingRowIndex(chat.items, thinking: streaming.thinking.isNotEmpty) - chat.rowBase}'), AgentText(streaming.content, url: widget.url, streaming: true)),
      for (final (i, row) in chatRowsList.indexed.toList().reversed)
        (
          ValueKey('row-${i - chat.rowBase}'),
          switch (row) {
            ItemRow(:final item) => ChatItemView(item, url: widget.url),
            StepsRow() => StepsView(row: row, url: widget.url),
            WorkerRow() => WorkerCard(row: row, url: widget.url),
          },
        ),
      if (chat.more) (const ValueKey('older'), OlderMessages(url: widget.url, session: chat.session, loading: chat.loadingOlder)),
    ];
    final keys = {for (final (i, (key, _)) in rows.indexed) key: i};

    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      if (widget.header)
        ChatHeader(
          sessionId: chat.session,
          session: session,
          parent: parent,
          onClose: widget.onClose,
          onBrowse: () => openPage(context, widget.url, 'site://home'),
          menu: ChatMenu(url: widget.url, session: session),
          // A worker's project is its worktree: no chips to change it.
          chips: session == null || session.ticket != null
              ? null
              : [
                  ProjectChip(url: widget.url, project: session.project),
                  if (session.project != null) HarnessChip(url: widget.url, project: session.project!),
                ],
        ),
      if (!widget.compact) OrchestrationBar(url: widget.url, status: widget.status, session: chat.session),
      Expanded(
        child: rows.isEmpty
            ? Center(child: Text(chat.loaded && chat.lastSeq == 0 ? 'Say something to start.' : '', style: const TextStyle(color: t.faint)))
            : Stack(children: [
                NotificationListener<ScrollNotification>(
                  onNotification: _onScroll,
                  child: SelectionArea(
                    child: ListView.builder(
                      key: PageStorageKey('chat-${widget.url}-${chat.session}${widget.compact ? '-side' : ''}'),
                      controller: _controller,
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
                if (chat.newSince > 0)
                  Align(
                    alignment: Alignment.bottomCenter,
                    child: Padding(
                      padding: const EdgeInsets.only(bottom: 4),
                      child: NewContentPill(key: const Key('new-content'), count: chat.newSince, onTap: _backToLatest),
                    ),
                  ),
              ]),
      ),
      if (!widget.compact) MessageComposer(key: ValueKey('composer-${widget.url}-${chat.session}'), url: widget.url, busy: busy),
    ]);
  }
}
