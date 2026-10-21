import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../../state/types.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../store_scope.dart';
import 'navigation.dart';
import 'review_view.dart';

/// Opens a ticket's review: its changes against the project's branch, Merge and Discard. In
/// the side pane's Changes tab on wide screens, as its own screen on phones.
void openReview(BuildContext context, String url, String ticket) {
  final store = StoreScope.read(context);
  if (isWide(context)) {
    store.dispatch(SideTabSelected(url, SideTab.changes, ticket: ticket));
    return;
  }
  store.dispatch(ReviewRequested(url, ticket));
  Navigator.of(context).push(MaterialPageRoute<void>(builder: (_) => ReviewPage(url: url, ticket: ticket)));
}

/// A ticket's review as its own screen (phones).
class ReviewPage extends StatelessWidget {
  const ReviewPage({super.key, required this.url, required this.ticket});

  final String url;
  final String ticket;

  @override
  Widget build(BuildContext context) {
    final store = StoreScope.of(context);
    final review = store.state.statusOf(url).reviews[ticket] ?? const TicketReviewState(busy: true);
    final ticketReview = review.changes?.ticket;
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
      ),
      body: ReviewView(url: url, ticket: ticket),
    );
  }
}
