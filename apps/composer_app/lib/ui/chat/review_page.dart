import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../../state/types.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../store_scope.dart';

/// Opens a ticket's review: its changes against the project's branch, Merge and Discard.
void openReview(BuildContext context, String url, String ticket) {
  StoreScope.read(context).dispatch(ReviewRequested(url, ticket));
  Navigator.of(context).push(MaterialPageRoute<void>(builder: (_) => ReviewPage(url: url, ticket: ticket)));
}

/// A ticket's branch: where it goes, its commits, the changed files and the diff.
class ReviewPage extends StatelessWidget {
  const ReviewPage({super.key, required this.url, required this.ticket});

  final String url;
  final String ticket;

  @override
  Widget build(BuildContext context) {
    final store = StoreScope.of(context);
    final review = store.state.statusOf(url).reviews[ticket] ?? const TicketReviewState(busy: true);
    final changes = review.changes;
    final ticketReview = changes?.ticket;
    final closed = review.merged != null || review.discarded;
    final canMerge = !review.busy &&
        !closed &&
        ticketReview != null &&
        ticketReview.target != null &&
        ticketReview.conflicts.isEmpty &&
        ticketReview.ahead > 0;
    return Scaffold(
      appBar: AppBar(
        titleSpacing: 0,
        title: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Text('Ticket $ticket', style: mono(size: 15, color: t.text, weight: FontWeight.w600)),
          if (ticketReview != null)
            Text(
              '${ticketReview.branch} → ${ticketReview.target ?? 'detached HEAD'}',
              style: mono(size: 11.5, color: t.muted),
            ),
        ]),
        actions: [
          IconButton(
            tooltip: 'Reload',
            onPressed: review.busy ? null : () => store.dispatch(ReviewRequested(url, ticket)),
            icon: const Icon(Icons.refresh, size: 20, color: t.muted),
          ),
        ],
        bottom: review.busy ? const PreferredSize(preferredSize: Size.fromHeight(2), child: LinearProgressIndicator(minHeight: 2)) : null,
      ),
      body: Column(children: [
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
              for (final line in changes.diff.split('\n')) _diffLine(line),
            ],
          ]),
        ),
        if (!closed && ticketReview != null)
          SafeArea(
            top: false,
            child: Container(
              padding: const EdgeInsets.fromLTRB(12, 8, 12, 8),
              decoration: const BoxDecoration(border: Border(top: BorderSide(color: t.border))),
              child: Row(children: [
                Expanded(
                  child: FilledButton(
                    key: const Key('merge-ticket'),
                    onPressed: canMerge ? () => store.dispatch(TicketMergeRequested(url, ticket)) : null,
                    child: Text('Merge into ${ticketReview.target ?? '…'}'),
                  ),
                ),
                const SizedBox(width: 8),
                OutlinedButton(
                  key: const Key('discard-ticket'),
                  style: OutlinedButton.styleFrom(foregroundColor: t.err),
                  onPressed: review.busy ? null : () => _confirmDiscard(context),
                  child: const Text('Discard'),
                ),
              ]),
            ),
          ),
      ]),
    );
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

  Widget _diffLine(String line) {
    final (color, background) = switch (line) {
      _ when line.startsWith('diff --git') => (t.text, t.raised),
      _ when line.startsWith('+++') || line.startsWith('---') => (t.muted, null),
      _ when line.startsWith('@@') => (t.info, null),
      _ when line.startsWith('+') => (t.ok, t.ok.withValues(alpha: 0.06)),
      _ when line.startsWith('-') => (t.err, t.err.withValues(alpha: 0.06)),
      _ => (t.muted, null),
    };
    return Container(
      color: background,
      padding: EdgeInsets.only(top: line.startsWith('diff --git') ? 4 : 0),
      child: Text(line.isEmpty ? ' ' : line, style: mono(size: 11.5, color: color)),
    );
  }

  Future<void> _confirmDiscard(BuildContext context) async {
    final store = StoreScope.read(context);
    final yes = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text('Discard ticket $ticket?'),
        content: const Text("Its worktree and branch are deleted, with every change on them. This can't be undone."),
        actions: [
          TextButton(onPressed: () => Navigator.pop(context, false), child: const Text('Keep')),
          TextButton(
            key: const Key('discard-confirm'),
            style: TextButton.styleFrom(foregroundColor: t.err),
            onPressed: () => Navigator.pop(context, true),
            child: const Text('Discard'),
          ),
        ],
      ),
    );
    if (yes ?? false) store.dispatch(TicketDiscardRequested(url, ticket));
  }
}
