import 'dart:math';

import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../../state/sites.dart';
import '../../state/workspace.dart';
import '../../theme/tokens.dart' as t;
import '../store_scope.dart';
import '../workspace_mark.dart';
import 'chat_menu.dart';
import 'chat_title.dart';
import 'chat_view.dart';
import 'live_view_rail.dart';
import 'live_view_section.dart';
import 'stop_button.dart';
import 'site_row.dart';

/// A chat as its own page (phones): the session's title and state with stop on top, the site
/// row, the live view of its application (when it has one), then the chat and the message bar.
/// The overflow menu holds the desk, pages, project, model and the rest.
///
/// In landscape on a phone ([sideBySide]) the chat goes left under one header row (the site
/// chip in it) and the live view right at full height; the view folds into a rail. With a live
/// view, Hide chat gives it the room. The chat stays built while hidden or moved (one key per
/// session), so its draft and scroll survive, and so does the loaded view.
class ChatPage extends StatefulWidget {
  static const _siteRow = 52.0;
  static const _handle = 44.0;

  /// What the chat and the message bar keep below the live view.
  static const _chatMinimum = 220.0;

  /// The chat column beside the live view: 44% of the width, at least this.
  static const _sideChatMinimum = 330.0;

  const ChatPage({super.key, required this.url, this.liveViewBuilder});

  final String url;

  /// What shows in the live view's box; the webview unless a test puts something else.
  final LiveViewBuilder? liveViewBuilder;

  /// Chat and live view side by side: a phone on its side (wider than tall, and short).
  static bool sideBySide(Size size) => size.width > size.height && size.height < 500;

  @override
  State<ChatPage> createState() => _ChatPageState();
}

class _ChatPageState extends State<ChatPage> {
  bool _chatHidden = false;
  bool _liveFolded = false;
  final _chatKeys = <String, GlobalKey>{};
  final _liveKeys = <String, GlobalKey>{};

  /// Hiding the chat unfolds the view; folding the view brings the chat back.
  void _hideChat(bool hidden) => setState(() {
        _chatHidden = hidden;
        if (hidden) _liveFolded = false;
      });

  void _fold(bool folded) => setState(() {
        _liveFolded = folded;
        if (folded) _chatHidden = false;
      });

  @override
  Widget build(BuildContext context) {
    final url = widget.url;
    final store = StoreScope.of(context);
    final status = store.state.statusOf(url);
    final chat = status.chat;
    final session = chat == null ? null : status.session(chat.session);
    final running = session != null && (session.state != TurnState.idle);
    final workspace = Workspace.of(url, info: status.info, entry: store.state.serverFor(url));
    final token = store.state.serverFor(url)?.token;
    final viewer = viewerPath(status.info);
    final app = status.liveViewOf(session);
    final hasLive = chat != null && app != null && viewer != null && token != null;
    final hidden = _chatHidden && hasLive;
    final side = ChatPage.sideBySide(MediaQuery.sizeOf(context));

    final actions = [
      if (running) StopButton(key: const Key('stop'), onPressed: () => store.dispatch(CancelRequested(url))),
      ChatMenu(url: url, session: session),
      const SizedBox(width: 2),
    ];
    final title = ChatTitle(status: status, session: session);

    if (chat == null) {
      return Scaffold(
        appBar: AppBar(toolbarHeight: 56, titleSpacing: 0, leadingWidth: 48, title: title, actions: actions),
        body: const Center(child: SizedBox(width: 18, height: 18, child: CircularProgressIndicator(strokeWidth: 2))),
      );
    }
    final chatView = ChatView(
      key: _chatKeys.putIfAbsent(chat.session, GlobalKey.new),
      url: url,
      status: status,
      chat: chat,
      stopInHeader: true,
    );
    final toggle = TextButton(
      key: const Key('chat-toggle'),
      style: TextButton.styleFrom(minimumSize: const Size(44, 44), padding: const EdgeInsets.symmetric(horizontal: 8), textStyle: const TextStyle(fontSize: 11)),
      onPressed: () => _hideChat(!hidden),
      child: Text(hidden ? 'Show chat' : 'Hide chat'),
    );
    Widget live({required bool fill, required double maxHeight, List<Widget> trailing = const []}) => LiveViewSection(
          key: _liveKeys.putIfAbsent(chat.session, GlobalKey.new),
          url: url,
          app: app!,
          viewer: viewer!,
          token: token!,
          maxHeight: maxHeight,
          fill: fill,
          trailing: [toggle, ...trailing],
          viewBuilder: widget.liveViewBuilder ?? LiveViewSection.webview,
        );

    if (side) {
      final canPop = ModalRoute.of(context)?.canPop ?? false;
      final header = Container(
        height: 48,
        color: t.surface,
        child: Row(children: [
          if (canPop) const SizedBox(width: 48, child: BackButton()) else const SizedBox(width: 14),
          Expanded(child: title),
          SiteRow(url: url, status: status, session: session, compact: true),
          ...actions,
        ]),
      );
      final chatColumn = Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        header,
        WorkspaceStrip(workspace: workspace),
        Expanded(child: chatView),
      ]);
      return Scaffold(
        body: SafeArea(
          child: LayoutBuilder(builder: (context, box) {
            final chatWidth = min(max(box.maxWidth * 0.44, ChatPage._sideChatMinimum), box.maxWidth - 200);
            return Row(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
              if (hidden)
                Offstage(child: SizedBox(width: chatWidth, height: box.maxHeight, child: chatColumn))
              else if (!hasLive || _liveFolded)
                Expanded(child: chatColumn)
              else
                SizedBox(width: chatWidth, child: chatColumn),
              if (hasLive)
                _liveFolded
                    ? LiveViewRail(key: const Key('live-unfold'), app: app, onTap: () => _fold(false))
                    : Expanded(
                        child: DecoratedBox(
                          decoration: BoxDecoration(border: Border(left: BorderSide(color: hidden ? Colors.transparent : t.border))),
                          child: live(fill: true, maxHeight: box.maxHeight, trailing: [
                            IconButton(
                              key: const Key('live-fold'),
                              tooltip: 'Fold the live view',
                              icon: const Icon(Icons.chevron_right, size: 20, color: t.muted),
                              onPressed: () => _fold(true),
                            ),
                          ]),
                        ),
                      ),
            ]);
          }),
        ),
      );
    }

    return Scaffold(
      appBar: AppBar(
        toolbarHeight: 56,
        titleSpacing: 0,
        leadingWidth: 48,
        title: title,
        actions: actions,
        bottom: PreferredSize(preferredSize: const Size.fromHeight(2), child: WorkspaceStrip(workspace: workspace)),
      ),
      body: LayoutBuilder(
        builder: (context, box) => Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          SiteRow(url: url, status: status, session: session),
          if (hasLive)
            hidden
                ? Expanded(child: live(fill: true, maxHeight: box.maxHeight))
                // The chat keeps room for the message bar and a few lines, keyboard or not.
                : live(fill: false, maxHeight: box.maxHeight - ChatPage._siteRow - ChatPage._handle - ChatPage._chatMinimum),
          if (hidden) Offstage(child: SizedBox(height: box.maxHeight, child: chatView)) else Expanded(child: chatView),
        ]),
      ),
    );
  }
}
