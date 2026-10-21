import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../../state/approval_text.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../store_scope.dart';

/// An approval waiting for you, answered in place; what it is about is one tap away.
class ApprovalTile extends StatelessWidget {
  const ApprovalTile({super.key, required this.url, required this.approval, this.agent});

  final String url;
  final ServerMessageApprovalRequest approval;

  /// The asking session's agent.
  final String? agent;

  @override
  Widget build(BuildContext context) {
    final store = StoreScope.read(context);
    void answer(ApprovalDecision decision) => store.dispatch(ApprovalAnswered(url, approval.request, decision));
    final asks = switch (approval.kind ?? ApprovalKind.recipe) {
      ApprovalKind.recipe => ' wants to run',
      ApprovalKind.merge => ' asks to merge',
      ApprovalKind.tool => ' wants to use a tool',
      ApprovalKind.amendment => ' proposes a change from your edits',
    };
    return Container(
      padding: const EdgeInsets.fromLTRB(12, 10, 12, 10),
      decoration: BoxDecoration(
        color: t.warn.withValues(alpha: 0.05),
        border: Border.all(color: t.warn.withValues(alpha: 0.4)),
        borderRadius: BorderRadius.circular(t.radius),
      ),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        Text.rich(TextSpan(style: const TextStyle(fontSize: 12, color: t.muted), children: [
          TextSpan(text: agent ?? 'a session'),
          TextSpan(text: ' #${approval.session}', style: mono(size: 11, color: t.faint)),
          TextSpan(text: asks),
        ])),
        const SizedBox(height: 4),
        Text(approvalHeadline(approval), style: mono(size: 13, color: t.text)),
        const SizedBox(height: 8),
        // Wraps in a narrow column (the wide layout's session list).
        Wrap(spacing: 8, runSpacing: 4, crossAxisAlignment: WrapCrossAlignment.center, children: [
          FilledButton(
            key: Key('approve-${approval.request}'),
            style: FilledButton.styleFrom(visualDensity: VisualDensity.compact),
            onPressed: () => answer(ApprovalDecision.approve),
            child: const Text('Approve'),
          ),
          if (approvalCanAlways(approval)) ...[
            OutlinedButton(
              style: OutlinedButton.styleFrom(visualDensity: VisualDensity.compact),
              onPressed: () => answer(ApprovalDecision.always),
              child: const Text('Always'),
            ),
          ],
          OutlinedButton(
            style: OutlinedButton.styleFrom(visualDensity: VisualDensity.compact),
            onPressed: () => answer(ApprovalDecision.deny),
            child: const Text('Deny'),
          ),
          TextButton(
            style: TextButton.styleFrom(foregroundColor: t.muted, visualDensity: VisualDensity.compact),
            onPressed: () => _showBody(context),
            child: const Text('View'),
          ),
        ]),
      ]),
    );
  }

  void _showBody(BuildContext context) => showModalBottomSheet<void>(
        context: context,
        showDragHandle: true,
        isScrollControlled: true,
        builder: (_) => SafeArea(
          child: Padding(
            padding: const EdgeInsets.fromLTRB(16, 0, 16, 16),
            child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: [
              Text(approvalHeadline(approval), style: mono(size: 14, color: t.text)),
              const SizedBox(height: 10),
              Container(
                constraints: const BoxConstraints(maxHeight: 360),
                padding: const EdgeInsets.all(10),
                decoration: BoxDecoration(color: t.bg, border: Border.all(color: t.border), borderRadius: BorderRadius.circular(t.radius)),
                child: SingleChildScrollView(child: SelectableText(approval.body, style: mono(size: 12, color: t.text))),
              ),
              const SizedBox(height: 8),
              Text(approvalNote(approval), style: const TextStyle(color: t.faint, fontSize: 12)),
            ]),
          ),
        ),
      );
}
