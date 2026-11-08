import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../../state/desk_view.dart';
import '../../state/workspace.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../pages/app_view.dart';
import '../pages/reader.dart';
import '../projects/project_overview_view.dart';
import '../store_scope.dart';
import '../workspace_mark.dart';
import 'message_composer.dart';
import 'selection_chip.dart';

/// A desk tab, full screen, with the chat bar still at the bottom: send without leaving it.
class DeskTabScreen extends StatefulWidget {
  const DeskTabScreen({super.key, required this.url, required this.entry});

  final String url;
  final DeskEntry entry;

  static void open(BuildContext context, String url, DeskEntry entry) =>
      Navigator.of(context).push(MaterialPageRoute<void>(builder: (_) => DeskTabScreen(url: url, entry: entry)));

  @override
  State<DeskTabScreen> createState() => _DeskTabScreenState();
}

class _DeskTabScreenState extends State<DeskTabScreen> {
  @override
  void initState() {
    super.initState();
    // Not during this build: the reader's state change rebuilds the tree.
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      if (widget.entry.tab case DeskTabPage(:final url)) {
        StoreScope.read(context).dispatch(PageRequested(widget.url, url));
      }
    });
  }

  @override
  Widget build(BuildContext context) {
    final store = StoreScope.of(context);
    final status = store.state.statusOf(widget.url);
    final chat = status.chat;
    final workspace = Workspace.of(widget.url, info: status.info, entry: store.state.serverFor(widget.url));
    final entry = widget.entry;
    final session = chat == null ? null : status.session(chat.session);
    final body = switch (entry.tab) {
      DeskTabApp(:final site, :final project) => AppView(
          key: ValueKey('desk-app-${entry.id}'),
          url: widget.url,
          token: store.state.serverFor(widget.url)?.token ?? '',
          site: site,
          project: project,
          session: chat?.session,
        ),
      DeskTabPage() => Reader(url: widget.url, status: status, onClose: null),
      DeskTabChanges(:final project) => ProjectOverviewView(url: widget.url, status: status, folder: project),
      _ => const SizedBox.shrink(),
    };
    return Scaffold(
      appBar: AppBar(
        titleSpacing: 0,
        title: Text(DeskView.label(entry), style: mono(size: 14, color: t.text, weight: FontWeight.w600)),
        bottom: PreferredSize(preferredSize: const Size.fromHeight(2), child: WorkspaceStrip(workspace: workspace)),
      ),
      body: body,
      bottomNavigationBar: chat == null
          ? null
          : Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                if (chat.selection case final selection?) SelectionChip(url: widget.url, selection: selection),
                MessageComposer(
                  key: ValueKey('desk-composer-${widget.url}-${chat.session}'),
                  url: widget.url,
                  busy: session != null && session.state != TurnState.idle,
                ),
              ],
            ),
    );
  }
}
