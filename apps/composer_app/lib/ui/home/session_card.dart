import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';

import '../../state/server_status.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../format.dart';
import 'activity_dot.dart';

/// A session in the Chats list: agent, state, last message, its project, and for an
/// orchestrator the workers it runs, one line each.
class SessionCard extends StatelessWidget {
  const SessionCard({super.key, required this.status, required this.session, required this.onTap, this.unread = false, this.selected = false, this.onWorker});

  final ServerStatus status;
  final SessionInfo session;
  final VoidCallback onTap;

  /// Finished something you haven't seen.
  final bool unread;

  /// The chat on screen (wide layout).
  final bool selected;

  /// A worker line was tapped; null: the card's tap.
  final void Function(SessionInfo worker)? onWorker;

  @override
  Widget build(BuildContext context) {
    final activity = status.activity(session);
    final workers = session.parent == null ? status.sessions.where((s) => s.parent == session.id).toList() : const <SessionInfo>[];
    final right = isBusy(activity) || const {'done', 'failed', 'paused'}.contains(activity)
        ? (activity == 'approval' ? 'needs approval' : activity)
        : (unread ? 'new · ${ago(session.updated)}' : ago(session.updated));
    final project = session.project?.split('/').last;
    return Material(
      color: selected ? t.raised : t.surface,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(t.radius),
        side: BorderSide(color: selected ? t.accent.withValues(alpha: 0.5) : t.border),
      ),
      child: InkWell(
        key: Key('session-${session.id}'),
        borderRadius: BorderRadius.circular(t.radius),
        onTap: onTap,
        child: Padding(
          padding: const EdgeInsets.fromLTRB(12, 10, 12, 10),
          child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
            Row(children: [
              ActivityDot(activity),
              const SizedBox(width: 8),
              Text(session.agent, style: mono(size: 13, color: t.text, weight: FontWeight.w500)),
              const SizedBox(width: 8),
              Text('#${session.id}', style: mono(size: 11, color: t.faint)),
              const Spacer(),
              Text(right, style: mono(size: 11, color: unread && !isBusy(activity) ? t.accent : (activity == 'idle' ? t.faint : activityColor(activity)))),
            ]),
            if (session.preview != null) ...[
              const SizedBox(height: 3),
              Text(_preview(session.preview!), maxLines: 1, overflow: TextOverflow.ellipsis, style: const TextStyle(fontSize: 13, color: t.muted)),
            ],
            if (workers.isNotEmpty)
              Container(
                margin: const EdgeInsets.only(top: 8, left: 3),
                padding: const EdgeInsets.only(left: 12),
                decoration: const BoxDecoration(border: Border(left: BorderSide(color: t.borderStrong))),
                child: Column(children: [
                  for (final w in workers)
                    InkWell(
                      key: Key('worker-line-${w.id}'),
                      onTap: onWorker == null ? null : () => onWorker!(w),
                      child: Padding(
                        padding: const EdgeInsets.symmetric(vertical: 2),
                        child: Row(children: [
                        ActivityDot(status.activity(w)),
                        const SizedBox(width: 8),
                        Text(w.ticket ?? '#${w.id}', style: mono(size: 12, color: t.text)),
                        const SizedBox(width: 8),
                        Expanded(
                          child: Text(
                            _workerLine(w),
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: const TextStyle(fontSize: 12, color: t.muted),
                          ),
                        ),
                        const SizedBox(width: 8),
                        Text(status.activity(w), style: mono(size: 11, color: activityColor(status.activity(w)))),
                        ]),
                      ),
                    ),
                ]),
              ),
            if (session.parent == null) ...[
              const SizedBox(height: 7),
              Row(children: [_Chip(project ?? 'no project')]),
            ],
          ]),
        ),
      ),
    );
  }

  /// `[worker #96a1 finished ticket 0001: done]` (the report as the server writes it) →
  /// `Worker #96a1 finished ticket 0001: done`.
  static String _preview(String preview) {
    final report = RegExp(r'^\[worker (#\S+ finished ticket \S+: \w+)\]').firstMatch(preview);
    return report == null ? preview : 'Worker ${report[1]}';
  }

  /// `ticket 0001: Note the release steps` → `Note the release steps`.
  static String _workerLine(SessionInfo w) {
    final preview = w.preview ?? '';
    final prefix = 'ticket ${w.ticket}: ';
    return preview.startsWith(prefix) ? preview.substring(prefix.length) : preview;
  }
}

class _Chip extends StatelessWidget {
  const _Chip(this.text);
  final String text;

  @override
  Widget build(BuildContext context) => Container(
        padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 1),
        decoration: BoxDecoration(border: Border.all(color: t.borderStrong), borderRadius: BorderRadius.circular(t.radiusSm)),
        child: Text(text, style: mono(size: 11)),
      );
}
