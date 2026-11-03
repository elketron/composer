import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../../state/types.dart';
import '../../theme/tokens.dart' as t;
import '../store_scope.dart';

/// Merge and Discard for a ticket's review; nothing once it is merged or discarded.
class ReviewActions extends StatelessWidget {
  const ReviewActions({super.key, required this.url, required this.ticket});

  final String url;
  final String ticket;

  @override
  Widget build(BuildContext context) {
    final store = StoreScope.of(context);
    final review = store.state.statusOf(url).reviews[ticket] ?? const TicketReviewState(busy: true);
    final ticketReview = review.changes?.ticket;
    final closed = review.merged != null || review.discarded;
    if (closed || ticketReview == null) return const SizedBox.shrink();
    final canMerge = !review.busy && ticketReview.target != null && ticketReview.conflicts.isEmpty && ticketReview.ahead > 0;
    return SafeArea(
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
