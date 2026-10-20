import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../home/activity_dot.dart';
import '../pages/page_navigation.dart';
import '../store_scope.dart';
import 'chat_view.dart';
import 'project_sheet.dart';
import 'review_page.dart';

/// A chat as its own page (phones): agent, state and project on top; ⋮ for the project,
/// compacting and stopping.
class ChatPage extends StatelessWidget {
  const ChatPage({super.key, required this.url});

  final String url;

  @override
  Widget build(BuildContext context) {
    final store = StoreScope.of(context);
    final status = store.state.statusOf(url);
    final chat = status.chat;
    final session = chat == null ? null : status.session(chat.session);
    final activity = session == null ? 'idle' : status.activity(session);
    final project = session?.project?.split('/').last;
    final switches = [
      for (final h in status.harnesses[session?.project] ?? const <HarnessSwitch>[])
        if (h.allowed && !h.entry.contains(':')) h,
    ];
    return Scaffold(
      appBar: AppBar(
        titleSpacing: 0,
        title: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Text(session?.agent ?? 'new chat', style: mono(size: 15, color: t.text, weight: FontWeight.w600)),
          if (chat != null)
            Row(children: [
              ActivityDot(activity, size: 6),
              const SizedBox(width: 6),
              Flexible(
                child: Text(
                  [activity == 'approval' ? 'needs approval' : activity, project ?? 'no project', '#${chat.session}'].join(' · '),
                  overflow: TextOverflow.ellipsis,
                  style: const TextStyle(fontSize: 11.5, color: t.muted),
                ),
              ),
            ]),
        ]),
        actions: [
          IconButton(
            tooltip: 'Pages',
            onPressed: () => openPage(context, url, status.page.page?.url ?? 'site://home'),
            icon: const Icon(Icons.description_outlined, size: 20, color: t.muted),
          ),
          PopupMenuButton<String>(
            key: const Key('chat-menu'),
            tooltip: 'More',
            icon: const Icon(Icons.more_vert, size: 20, color: t.muted),
            enabled: chat != null,
            onOpened: () {
              final folder = session?.project;
              if (folder != null) store.dispatch(HarnessesRequested(url, folder));
            },
            onSelected: (choice) => switch (choice) {
              _ when choice.startsWith('harness:') => _toggle(store, url, session?.project, choice),
              'review' => openReview(context, url, session!.ticket!),
              'project' => showProjectSheet(context, url),
              'unset' => store.dispatch(ProjectChosen(url, null)),
              'compact' => store.dispatch(CompactRequested(url)),
              'stop' => store.dispatch(CancelRequested(url)),
              _ => null,
            },
            itemBuilder: (_) => [
              if (session?.ticket != null) PopupMenuItem(value: 'review', child: Text('Review changes of ticket ${session!.ticket}')),
              PopupMenuItem(value: 'project', child: Text(project == null ? 'Set project…' : 'Change project…')),
              if (project != null) PopupMenuItem(value: 'unset', child: Text('Unset $project')),
              for (final h in switches)
                PopupMenuItem(
                  value: 'harness:${h.on ? 'off' : 'on'}:${h.entry}',
                  child: Text('${h.on ? 'Turn off' : 'Turn on'} ${harnessName(h.entry)} for $project'),
                ),
              const PopupMenuItem(value: 'compact', child: Text('Compact (save memory, summarize)')),
              if (session != null && session.state != TurnState.idle) const PopupMenuItem(value: 'stop', child: Text('Stop the turn')),
            ],
          ),
        ],
        bottom: const PreferredSize(preferredSize: Size.fromHeight(1), child: Divider()),
      ),
      body: chat == null
          ? const Center(child: SizedBox(width: 18, height: 18, child: CircularProgressIndicator(strokeWidth: 2)))
          : ChatView(url: url, status: status, chat: chat, header: false),
    );
  }

  /// `harness:off:claude-code` → turn it off for the session's project.
  void _toggle(AppStore store, String url, String? project, String choice) {
    final rest = choice.substring('harness:'.length);
    final colon = rest.indexOf(':');
    if (project == null || colon < 0) return;
    store.dispatch(HarnessToggled(url, project, rest.substring(colon + 1), rest.substring(0, colon) == 'on'));
  }
}

/// `claude-code` → `Claude Code`.
String harnessName(String entry) => switch (entry) {
      'claude-code' => 'Claude Code',
      _ => entry,
    };
