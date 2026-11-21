import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';

import '../../state/run_kind.dart';
import '../../state/server_status.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../format.dart';
import 'activity_dot.dart';
import 'site_tag.dart';

/// A session in the Inbox: its run kind, state, last message and site.
class SessionCard extends StatelessWidget {
  const SessionCard({super.key, required this.status, required this.session, required this.onTap, this.unread = false});

  final ServerStatus status;
  final SessionInfo session;
  final VoidCallback onTap;

  /// Finished something you haven't seen.
  final bool unread;

  @override
  Widget build(BuildContext context) {
    final activity = status.activity(session);
    final right = isBusy(activity)
        ? (activity == 'approval' ? 'needs approval' : activity)
        : (unread ? 'new · ${ago(session.updated)}' : ago(session.updated));
    return Material(
      color: t.surface,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(t.radius),
        side: const BorderSide(color: t.border),
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
              Text(runKindLabel(session.kind), style: mono(size: 13, color: t.text, weight: FontWeight.w500)),
              const SizedBox(width: 8),
              Text('#${session.id}', style: mono(size: 11, color: t.faint)),
              const Spacer(),
              Text(right, style: mono(size: 11, color: unread && !isBusy(activity) ? t.accent : (activity == 'idle' ? t.faint : activityColor(activity)))),
            ]),
            if (session.preview != null) ...[
              const SizedBox(height: 3),
              Text(session.preview!, maxLines: 1, overflow: TextOverflow.ellipsis, style: const TextStyle(fontSize: 13, color: t.muted)),
            ],
            const SizedBox(height: 7),
            Row(children: [
              if (session.site != null) SiteTag(session.site!, key: Key('site-tag-${session.id}')) else const _Chip('no site'),
            ]),
          ]),
        ),
      ),
    );
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
