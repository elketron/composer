import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';

import '../../state/types.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../store_scope.dart';
import 'diff_page.dart';
import 'review_actions.dart';

/// A ticket's branch: where it goes, its commits, the changed files and a button to its diff, with
/// Merge and Discard below. The phone's review screen and the wide layout's Changes tab.
class ReviewView extends StatelessWidget {
  const ReviewView({super.key, required this.url, required this.ticket, this.header = false});

  final String url;
  final String ticket;

  /// Show the branch and target on top (the phone's app bar shows them otherwise).
  final bool header;

  @override
  Widget build(BuildContext context) {
    final store = StoreScope.of(context);
    final review = store.state.statusOf(url).reviews[ticket] ?? const TicketReviewState(busy: true);
    final changes = review.changes;
    final ticketReview = changes?.ticket;
    final closed = review.merged != null || review.discarded;
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      if (header && ticketReview != null)
        Padding(
          padding: const EdgeInsets.fromLTRB(12, 10, 12, 0),
          child: Text('${ticketReview.branch} → ${ticketReview.target ?? 'detached HEAD'}', style: mono(size: 12, color: t.muted)),
        ),
      if (review.busy) const LinearProgressIndicator(minHeight: 2),
      Expanded(
        child: ListView(padding: const EdgeInsets.fromLTRB(12, 8, 12, 12), children: [
          if (review.merged != null) _banner('Merged into ${ticketReview?.target ?? 'the project'} as ${review.merged}. Its worktree and branch are removed.', t.ok),
          if (review.discarded) _banner('Discarded: its worktree and branch are removed.', t.warn),
          if (review.error != null) _banner(review.error!, t.err),
          if (ticketReview != null && !closed) ..._summary(ticketReview),
          if (changes != null && !closed) ...[
            for (final file in changes.files) _file(file),
            const SizedBox(height: 12),
            if (changes.files.isEmpty) const Text('No changes on its branch yet.', style: TextStyle(color: t.faint, fontSize: 13)),
            if (changes.diff.trim().isNotEmpty)
              OutlinedButton(
                key: const Key('view-diff'),
                onPressed: () => openDiff(context, url, ticket),
                child: const Text('View diff'),
              ),
          ],
        ]),
      ),
      ReviewActions(url: url, ticket: ticket),
    ]);
  }

  List<Widget> _summary(TicketReview review) => [
        Text.rich(TextSpan(style: const TextStyle(fontSize: 13, color: t.muted), children: [
          TextSpan(text: '${review.commits.length} commit${review.commits.length == 1 ? '' : 's'}'),
          if (review.uncommitted) const TextSpan(text: ' · uncommitted changes (committed when merged)', style: TextStyle(color: t.warn)),
        ])),
        const SizedBox(height: 4),
        for (final commit in review.commits) Text(commit, style: mono(size: 12, color: t.faint)),
        if (review.conflicts.isNotEmpty) ...[
          const SizedBox(height: 8),
          _banner(
            'Conflicts with ${review.target} in ${review.conflicts.join(', ')}. Ask its worker to merge ${review.target} into its branch and resolve them.',
            t.err,
          ),
        ],
        const SizedBox(height: 12),
      ];

  Widget _banner(String text, Color color) => Container(
        margin: const EdgeInsets.only(bottom: 10),
        padding: const EdgeInsets.all(10),
        decoration: BoxDecoration(
          color: color.withValues(alpha: 0.06),
          border: Border.all(color: color.withValues(alpha: 0.4)),
          borderRadius: BorderRadius.circular(t.radius),
        ),
        child: Text(text, style: TextStyle(fontSize: 13, color: color)),
      );

  Widget _file(ChangedFile file) {
    final color = switch (file.status) { 'A' || '?' => t.ok, 'D' => t.err, _ => t.warn };
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 2),
      child: Row(children: [
        SizedBox(width: 18, child: Text(file.status, style: mono(size: 12, color: color))),
        Expanded(child: Text(file.path, overflow: TextOverflow.ellipsis, style: mono(size: 12, color: t.text))),
        Text('+${file.added}', style: mono(size: 12, color: t.ok)),
        const SizedBox(width: 6),
        Text('-${file.removed}', style: mono(size: 12, color: t.err)),
      ]),
    );
  }
}
