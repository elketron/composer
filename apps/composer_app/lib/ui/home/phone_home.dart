import 'package:flutter/material.dart';

import '../../state/works.dart';
import '../../state/server_entry.dart';
import '../../state/server_status.dart';
import '../../state/types.dart';
import '../../state/workspace.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../approval_bar.dart';
import '../projects/project_list_view.dart';
import '../projects/project_screen.dart';
import '../server_list.dart';
import '../server_list_footer.dart';
import '../status_dot.dart';
import '../store_scope.dart';
import '../workspace_mark.dart';
import 'chats_tab.dart';
import 'pages_tab.dart';
import 'phone_tabs.dart';
import 'server_tab.dart';

/// The phone layout: the server switcher on top, Chats / Pages / Server below.
class PhoneHome extends StatelessWidget {
  const PhoneHome({super.key, required this.server, required this.status});

  final ServerEntry server;
  final ServerStatus status;

  @override
  Widget build(BuildContext context) {
    final state = StoreScope.of(context).state;
    final tab = state.tab;
    final unread = status.sessions.where((s) => status.unread.contains(s.id)).length;
    final workspace = Workspace.of(server.url, info: status.info, entry: server);
    // Tells same-named servers apart: `my-pc` on `tail1234` (a tailnet's name; not for IPs).
    final host = Uri.parse(server.url).host.split('.');
    final where = host.length > 2 && RegExp('[a-z]').hasMatch(host.first) ? host[1] : '';
    return Scaffold(
      appBar: AppBar(
        titleSpacing: 8,
        title: TextButton(
          key: const Key('switch-server'),
          style: TextButton.styleFrom(foregroundColor: t.text),
          onPressed: () => _openSwitcher(context),
          child: Row(mainAxisSize: MainAxisSize.min, children: [
            WorkspaceMark(
              key: const Key('active-workspace'),
              workspace: workspace,
              dotSize: 10,
              style: const TextStyle(fontSize: 16, fontWeight: FontWeight.w600),
            ),
            const SizedBox(width: 8),
            StatusDot(status.link),
            if (where.isNotEmpty) ...[
              const SizedBox(width: 6),
              Text(where, style: mono(size: 11, color: t.faint)),
            ],
            const SizedBox(width: 2),
            const Icon(Icons.expand_more, size: 18, color: t.muted),
          ]),
        ),
        bottom: PreferredSize(preferredSize: const Size.fromHeight(2), child: WorkspaceStrip(workspace: workspace)),
      ),
      body: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        // Approvals are answered in the Chats list itself; elsewhere the bar points at them.
        if (tab != PhoneTab.chats) const ApprovalBar(),
        Expanded(
          child: switch (tab) {
            PhoneTab.chats => ChatsTab(key: ValueKey('chats ${server.url}'), url: server.url, status: status),
            PhoneTab.projects => ProjectListView(
                key: ValueKey('projects ${server.url}'),
                url: server.url,
                status: status,
                onPick: (p) => ProjectScreen.open(context, server.url, p.folder ?? p.path),
              ),
            PhoneTab.pages => PagesTab(key: ValueKey('pages ${server.url}'), url: server.url, status: status),
            PhoneTab.server => ServerTab(server: server, status: status),
          },
        ),
      ]),
      bottomNavigationBar: PhoneTabs(
        tab: tab,
        needsYou: status.approvals.length + unread + (status.pipelines?.works.where((w) => waitingStage(w) != null).length ?? 0),
      ),
    );
  }

  void _openSwitcher(BuildContext context) => showModalBottomSheet<void>(
        context: context,
        showDragHandle: true,
        builder: (sheet) => SafeArea(
          child: SizedBox(
            height: MediaQuery.sizeOf(context).height * 0.5,
            child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
              Expanded(child: SingleChildScrollView(child: ServerList(onSelected: () => Navigator.pop(sheet)))),
              const ServerListFooter(),
            ]),
          ),
        ),
      );
}
