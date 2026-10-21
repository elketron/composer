import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../../theme/tokens.dart' as t;
import '../store_scope.dart';
import 'project_sheet.dart';
import 'review_page.dart';

/// ⋮ for the open chat: its project, a worker's changes, Claude Code on or off for the project,
/// compacting and stopping. The same on phones (app bar) and wide screens (chat header).
class ChatMenu extends StatelessWidget {
  const ChatMenu({super.key, required this.url, required this.session});

  final String url;
  final SessionInfo? session;

  @override
  Widget build(BuildContext context) {
    final store = StoreScope.of(context);
    final status = store.state.statusOf(url);
    final session = this.session;
    final project = session?.project?.split('/').last;
    final switches = [
      for (final h in status.harnesses[session?.project] ?? const <HarnessSwitch>[])
        if (h.allowed && !h.entry.contains(':')) h,
    ];
    return PopupMenuButton<String>(
      key: const Key('chat-menu'),
      tooltip: 'More',
      icon: const Icon(Icons.more_vert, size: 20, color: t.muted),
      enabled: session != null,
      onOpened: () {
        final folder = session?.project;
        if (folder != null) store.dispatch(HarnessesRequested(url, folder));
      },
      onSelected: (choice) => switch (choice) {
        _ when choice.startsWith('harness:') => _toggle(store, session?.project, choice),
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
    );
  }

  /// `harness:off:claude-code` → turn it off for the session's project.
  void _toggle(AppStore store, String? project, String choice) {
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
