import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../../state/server_status.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../store_scope.dart';

/// The open session's orchestration, one tap per session: the orchestrator, then each
/// worker with its ticket and what it is doing. Hidden for sessions without workers.
class OrchestrationBar extends StatelessWidget {
  const OrchestrationBar({super.key, required this.url, required this.status, required this.session});

  final String url;
  final ServerStatus status;

  /// The open one.
  final String session;

  @override
  Widget build(BuildContext context) {
    final members = status.orchestration(session);
    if (members.isEmpty) return const SizedBox.shrink();
    return Container(
      height: 36,
      decoration: const BoxDecoration(color: t.surface, border: Border(bottom: BorderSide(color: t.border))),
      child: ListView(
        scrollDirection: Axis.horizontal,
        padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 5),
        children: [
          for (final s in members) ...[
            _chip(context, s, s.id == session),
            const SizedBox(width: 6),
          ],
        ],
      ),
    );
  }

  Widget _chip(BuildContext context, SessionInfo s, bool open) {
    final activity = status.activity(s);
    final (dot, label) = switch (activity) {
      'running' || 'waiting' || 'starting' => (t.warn, activity),
      'approval' => (t.warn, 'needs approval'),
      'done' => (t.ok, 'done'),
      'failed' => (t.err, 'failed'),
      _ => (t.faint, 'idle'),
    };
    return Tooltip(
      message: '${s.agent} #${s.id} · $label',
      child: Material(
        color: open ? t.raised : Colors.transparent,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(t.radius),
          side: BorderSide(color: open ? t.borderStrong : t.border),
        ),
        child: InkWell(
          key: Key('orchestration-${s.id}'),
          borderRadius: BorderRadius.circular(t.radius),
          onTap: open ? null : () => StoreScope.read(context).dispatch(SessionOpened(url, s.id)),
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 9),
            child: Row(mainAxisSize: MainAxisSize.min, children: [
              Container(width: 7, height: 7, decoration: BoxDecoration(color: dot, shape: BoxShape.circle)),
              const SizedBox(width: 7),
              Text(
                s.parent == null ? s.agent : (s.ticket != null ? 'ticket ${s.ticket}' : s.agent),
                style: mono(size: 12, color: open ? t.text : t.muted, weight: open ? FontWeight.w600 : FontWeight.w400),
              ),
              Text(
                activity == 'idle' ? '  #${s.id}' : '  $label',
                style: mono(size: 11, color: activity == 'idle' ? t.faint : dot),
              ),
            ]),
          ),
        ),
      ),
    );
  }
}
