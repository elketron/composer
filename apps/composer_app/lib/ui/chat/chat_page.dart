import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../../state/chat_rows.dart';
import '../../state/workflows.dart';
import '../../state/workspace.dart';
import '../../theme/tokens.dart' as t;
import '../home/activity_dot.dart';
import '../store_scope.dart';
import '../workspace_mark.dart';
import 'chat_menu.dart';
import 'chat_view.dart';
import 'live_view_section.dart';
import 'stop_button.dart';
import 'workflow_row.dart';

/// A chat as its own page (phones): the session's title and state with stop on top, the workflow
/// row, the live view of its application (when it has one), then the chat and the message bar.
/// The overflow menu holds the desk, pages, project, model and the rest.
class ChatPage extends StatelessWidget {
  static const _workflowRow = 52.0;
  static const _handle = 44.0;

  /// What the chat and the message bar keep below the live view.
  static const _chatMinimum = 220.0;

  const ChatPage({super.key, required this.url, this.liveViewBuilder});

  final String url;

  /// What shows in the live view's box; the webview unless a test puts something else.
  final LiveViewBuilder? liveViewBuilder;

  @override
  Widget build(BuildContext context) {
    final store = StoreScope.of(context);
    final status = store.state.statusOf(url);
    final chat = status.chat;
    final session = chat == null ? null : status.session(chat.session);
    final activity = session == null ? 'idle' : status.activity(session);
    final running = session != null && (session.state != TurnState.idle);
    final workspace = Workspace.of(url, info: status.info, entry: store.state.serverFor(url));
    final token = store.state.serverFor(url)?.token;
    final viewer = viewerPath(status.info);
    final app = status.liveViewOf(session);
    return Scaffold(
      appBar: AppBar(
        toolbarHeight: 56,
        titleSpacing: 0,
        leadingWidth: 48,
        title: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Text(
            session?.agent ?? 'new chat',
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: const TextStyle(fontSize: 15, fontWeight: FontWeight.w600),
          ),
          if (chat != null)
            Row(children: [
              ActivityDot(activity),
              const SizedBox(width: 6),
              Flexible(
                child: Text(
                  '${stateLine(activity, stepsSinceLastMessage(chat.items))} · #${chat.session}',
                  key: const Key('session-state'),
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: const TextStyle(fontSize: 11, color: t.muted),
                ),
              ),
            ]),
        ]),
        actions: [
          if (running) StopButton(key: const Key('stop'), onPressed: () => store.dispatch(CancelRequested(url))),
          ChatMenu(url: url, session: session),
          const SizedBox(width: 2),
        ],
        bottom: PreferredSize(preferredSize: const Size.fromHeight(2), child: WorkspaceStrip(workspace: workspace)),
      ),
      body: chat == null
          ? const Center(child: SizedBox(width: 18, height: 18, child: CircularProgressIndicator(strokeWidth: 2)))
          : LayoutBuilder(
              builder: (context, box) => Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
                WorkflowRow(url: url, status: status, session: session),
                if (app != null && viewer != null && token != null)
                  // The chat keeps room for the message bar and a few lines, keyboard or not.
                  LiveViewSection(
                    key: ValueKey('live-view-section-${chat.session}'),
                    url: url,
                    app: app,
                    viewer: viewer,
                    token: token,
                    maxHeight: box.maxHeight - _workflowRow - _handle - _chatMinimum,
                    viewBuilder: liveViewBuilder ?? LiveViewSection.webview,
                  ),
                Expanded(child: ChatView(url: url, status: status, chat: chat, stopInHeader: true)),
              ]),
            ),
    );
  }
}
