import 'package:flutter/material.dart';

import '../../state/chat_items.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../pages/page_navigation.dart';
import 'review_page.dart';

/// A worker's report to its orchestrator: which ticket, how it went, and its Result.
class WorkerCard extends StatelessWidget {
  const WorkerCard({super.key, required this.row, required this.url});

  final WorkerRow row;

  /// The server.
  final String url;

  @override
  Widget build(BuildContext context) {
    final color = switch (row.status) { 'done' => t.ok, 'paused' => t.warn, _ => t.err };
    return Container(
      padding: const EdgeInsets.fromLTRB(10, 8, 4, 8),
      decoration: BoxDecoration(
        color: color.withValues(alpha: 0.04),
        border: Border.all(color: color.withValues(alpha: 0.3)),
        borderRadius: BorderRadius.circular(t.radius),
      ),
      child: Row(children: [
        Container(width: 7, height: 7, decoration: BoxDecoration(color: color, shape: BoxShape.circle)),
        const SizedBox(width: 10),
        Expanded(
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Text.rich(TextSpan(style: const TextStyle(fontSize: 13, fontWeight: FontWeight.w500), children: [
              TextSpan(text: 'Worker #${row.worker} ${switch (row.status) { 'done' => 'finished', 'paused' => 'paused on', _ => 'failed' }} ticket ${row.ticket}'),
            ])),
            if (row.summary.isNotEmpty)
              Text(row.summary, maxLines: 3, overflow: TextOverflow.ellipsis, style: const TextStyle(fontSize: 12, color: t.muted)),
          ]),
        ),
        if (row.branch != null)
          TextButton(
            key: Key('review-${row.ticket}'),
            style: TextButton.styleFrom(foregroundColor: t.muted, visualDensity: VisualDensity.compact),
            onPressed: () => openReview(context, url, row.ticket),
            child: Text('Changes', style: mono(size: 12, color: t.accent)),
          ),
        if (row.result != null)
          TextButton(
            style: TextButton.styleFrom(foregroundColor: t.muted, visualDensity: VisualDensity.compact),
            onPressed: () => openPage(context, url, row.result!),
            child: Text('Result', style: mono(size: 12, color: t.accent)),
          ),
      ]),
    );
  }
}
