import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../../state/server_entry.dart';
import '../../state/server_status.dart';
import '../../theme/tokens.dart' as t;
import '../approval_bar.dart';
import '../chat/chat_view.dart';
import '../home/new_chat_sheet.dart';
import '../store_scope.dart';
import 'server_rail.dart';
import 'session_column.dart';
import 'session_strip.dart';
import 'side_pane.dart';

/// Desktop and tablets: server rail, session list (or its folded strip), the chat, and the side
/// pane beside it, or over it when the window is narrow.
class WideLayout extends StatefulWidget {
  const WideLayout({super.key, required this.server, required this.status, required this.width});

  /// Below this the side pane covers the chat instead of sitting beside it.
  static const sideBySide = 1100.0;

  final ServerEntry server;
  final ServerStatus status;
  final double width;

  @override
  State<WideLayout> createState() => _WideLayoutState();
}

class _WideLayoutState extends State<WideLayout> {
  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _openLatest();
  }

  @override
  void didUpdateWidget(WideLayout old) {
    super.didUpdateWidget(old);
    _openLatest();
  }

  /// Opens on your latest chat, once per server: not again after you close it.
  void _openLatest() {
    final status = widget.status;
    if (status.opened || status.chat != null) return;
    final latest = status.sessions.where((s) => s.parent == null).firstOrNull;
    if (latest == null) return;
    final url = widget.server.url;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) StoreScope.read(context).dispatch(SessionOpened(url, latest.id));
    });
  }

  @override
  Widget build(BuildContext context) {
    final store = StoreScope.of(context);
    final layout = store.state.layout;
    final server = widget.server;
    final status = widget.status;
    final chat = status.chat;
    final collapsed = layout.sessionsCollapsed;
    final left = ServerRail.width + (collapsed ? SessionStrip.width : SessionColumn.width);
    final beside = widget.width >= WideLayout.sideBySide;
    final room = widget.width - left - (beside ? 380 : 40);
    final sideWidth = layout.sideWidth.clamp(SidePane.minWidth, room < SidePane.minWidth ? SidePane.minWidth : room).toDouble();
    final side = status.side == null
        ? null
        : SidePane(key: ValueKey('side-${server.url}'), server: server, status: status, width: sideWidth, maxWidth: room);

    final main = chat == null
        ? _NoChat(onNew: () => showNewChat(context, server.url))
        : ChatView(
            key: ValueKey('${server.url} ${chat.session}'),
            url: server.url,
            status: status,
            chat: chat,
            onClose: () => store.dispatch(SessionClosed(server.url)),
          );

    return Scaffold(
      body: Row(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        const ServerRail(),
        collapsed ? SessionStrip(url: server.url, status: status) : SessionColumn(server: server, status: status),
        Expanded(
          child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
            // The session list shows approvals itself; folded, the bar does.
            if (collapsed) const ApprovalBar(),
            Expanded(
              child: beside
                  ? Row(crossAxisAlignment: CrossAxisAlignment.stretch, children: [Expanded(child: main), ?side])
                  : Stack(children: [
                      Positioned.fill(child: main),
                      if (side != null)
                        Positioned(
                          top: 0,
                          bottom: 0,
                          right: 0,
                          child: Material(elevation: 12, color: t.bg, child: side),
                        ),
                    ]),
            ),
          ]),
        ),
      ]),
    );
  }
}

class _NoChat extends StatelessWidget {
  const _NoChat({required this.onNew});
  final VoidCallback onNew;

  @override
  Widget build(BuildContext context) => Center(
        child: Column(mainAxisSize: MainAxisSize.min, children: [
          const Text('Pick a chat on the left, or start one.', style: TextStyle(color: t.muted)),
          const SizedBox(height: 12),
          FilledButton.icon(onPressed: onNew, icon: const Icon(Icons.add, size: 18), label: const Text('New chat')),
        ]),
      );
}
