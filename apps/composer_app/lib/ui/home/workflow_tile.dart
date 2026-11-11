import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';

import '../../state/server_status.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../chat/navigation.dart';
import 'activity_dot.dart';
import 'new_chat_sheet.dart';

/// One workflow: its name and title, its application and MCP servers, how many sessions are in it
/// now (listed under it, tap to open), and an action to start a chat in it.
class WorkflowTile extends StatelessWidget {
  const WorkflowTile({super.key, required this.url, required this.status, required this.site});

  final String url;
  final ServerStatus status;
  final SiteSummary site;

  @override
  Widget build(BuildContext context) {
    final sessions = status.sessionsIn(site.name);
    Widget fact(String label, String value) => Padding(
          padding: const EdgeInsets.only(top: 3),
          child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
            SizedBox(width: 44, child: Text(label, style: mono(size: 11, color: t.faint))),
            Expanded(child: Text(value, style: mono(size: 12, color: t.text))),
          ]),
        );
    return Container(
      key: Key('workflow-${site.name}'),
      decoration: BoxDecoration(color: t.surface, border: Border.all(color: t.border), borderRadius: BorderRadius.circular(t.radiusCard)),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(12, 10, 6, 8),
          child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Expanded(
              child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                Text.rich(TextSpan(children: [
                  TextSpan(text: site.name, style: mono(size: 14, color: t.accent, weight: FontWeight.w600)),
                  if (site.title.isNotEmpty && site.title != site.name) TextSpan(text: '  ${site.title}', style: const TextStyle(fontSize: 13, color: t.muted)),
                ])),
                if (site.app != null && site.app!.isNotEmpty) fact('app', site.app!),
                if (site.mcp.isNotEmpty) fact('mcp', site.mcp.join(', ')),
                fact('in it', sessions.isEmpty ? 'no sessions' : '${sessions.length} session${sessions.length == 1 ? '' : 's'}'),
              ]),
            ),
            Tooltip(
              message: 'New chat in ${site.name}',
              child: Material(
                color: t.raised,
                shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(t.radiusControl), side: const BorderSide(color: t.borderStrong)),
                child: InkWell(
                  key: Key('workflow-new-${site.name}'),
                  borderRadius: BorderRadius.circular(t.radiusControl),
                  onTap: () => showNewChat(context, url, site: site.name),
                  child: const SizedBox(width: 44, height: 44, child: Icon(Icons.add, size: 20, color: t.accent)),
                ),
              ),
            ),
          ]),
        ),
        for (final s in sessions)
          InkWell(
            key: Key('workflow-session-${s.id}'),
            onTap: () => openSession(context, url, s.id),
            child: Container(
              constraints: const BoxConstraints(minHeight: 44),
              padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 6),
              decoration: const BoxDecoration(border: Border(top: BorderSide(color: t.border))),
              child: Row(children: [
                ActivityDot(status.activity(s)),
                const SizedBox(width: 8),
                Text(s.agent, style: mono(size: 12, color: t.text)),
                const SizedBox(width: 6),
                Text('#${s.id}', style: mono(size: 11, color: t.faint)),
                const SizedBox(width: 8),
                Expanded(child: Text(s.preview ?? '', maxLines: 1, overflow: TextOverflow.ellipsis, style: const TextStyle(fontSize: 12, color: t.muted))),
              ]),
            ),
          ),
      ]),
    );
  }
}
