import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';

import '../../state/server_status.dart';
import '../../state/works.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../pages/page_navigation.dart';

/// A work whose stage waits for your review (Needs you): opens its pipeline's board, or the
/// work's page where the board isn't built.
class GateTile extends StatelessWidget {
  const GateTile({super.key, required this.url, required this.status, required this.work});

  final String url;
  final ServerStatus status;
  final WorkSummary work;

  @override
  Widget build(BuildContext context) {
    final stage = waitingStage(work);
    final site = workSite(work.url);
    return Padding(
      padding: const EdgeInsets.only(bottom: 8),
      child: Material(
        color: t.accent.withValues(alpha: 0.06),
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(t.radius),
          side: BorderSide(color: t.accent.withValues(alpha: 0.4)),
        ),
        child: InkWell(
          key: Key('gate-${work.url}'),
          borderRadius: BorderRadius.circular(t.radius),
          onTap: () => status.hasApp(site) ? openApp(context, url, site) : openPage(context, url, work.url),
          child: Padding(
            padding: const EdgeInsets.all(11),
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Row(children: [
                Container(width: 7, height: 7, decoration: const BoxDecoration(color: t.accent, shape: BoxShape.circle)),
                const SizedBox(width: 8),
                Expanded(child: Text(work.title, maxLines: 1, overflow: TextOverflow.ellipsis, style: mono(size: 12.5, color: t.text, weight: FontWeight.w500))),
                Text(site, style: mono(size: 11, color: t.faint)),
              ]),
              const SizedBox(height: 3),
              Text('${stage?.name.replaceFirst('_', ' ') ?? ''} waits for review', style: mono(size: 11.5, color: t.accent)),
              if (stage?.summary != null) ...[
                const SizedBox(height: 4),
                Text(stage!.summary!, maxLines: 2, overflow: TextOverflow.ellipsis, style: const TextStyle(fontSize: 12, color: t.muted)),
              ],
            ]),
          ),
        ),
      ),
    );
  }
}
