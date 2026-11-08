import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../../state/approval_text.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../store_scope.dart';

/// A recipe, a merge or a tool waiting for your answer: what it is, and Approve / Deny (and
/// Always, for a tool with a rule).
class ApprovalCard extends StatelessWidget {
  const ApprovalCard({super.key, required this.url, required this.approval, this.serverName});

  final String url;
  final ServerMessageApprovalRequest approval;

  /// Shown when approvals from several servers are listed together.
  final String? serverName;

  @override
  Widget build(BuildContext context) {
    final store = StoreScope.read(context);
    final command = approvalHeadline(approval);
    void answer(ApprovalDecision decision) => store.dispatch(ApprovalAnswered(url, approval.request, decision));
    return Container(
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: t.surface,
        border: Border.all(color: t.warn),
        borderRadius: BorderRadius.circular(t.radius),
      ),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        Text.rich(TextSpan(children: [
          TextSpan(text: '${approvalQuestion(approval)} ', style: const TextStyle(color: t.warn, fontSize: 14)),
          TextSpan(text: command, style: mono(size: 13, color: t.text)),
          TextSpan(text: '  #${approval.session}${serverName == null ? '' : ' on $serverName'}', style: mono(size: 12, color: t.faint)),
        ])),
        Container(
          margin: const EdgeInsets.symmetric(vertical: 10),
          padding: const EdgeInsets.all(8),
          constraints: const BoxConstraints(maxHeight: 240),
          decoration: BoxDecoration(color: t.bg, border: Border.all(color: t.border), borderRadius: BorderRadius.circular(t.radius)),
          child: SingleChildScrollView(child: SelectableText(approval.body, style: mono(size: 12, color: t.text))),
        ),
        Text(approvalNote(approval), style: const TextStyle(color: t.faint, fontSize: 12)),
        const SizedBox(height: 10),
        Row(children: [
          Expanded(
            child: FilledButton(
              key: Key('approve-${approval.request}'),
              onPressed: () => answer(ApprovalDecision.approve),
              child: const Text('Approve'),
            ),
          ),
          if (approvalCanAlways(approval)) ...[
            const SizedBox(width: 8),
            Expanded(
              child: OutlinedButton(
                key: Key('always-${approval.request}'),
                onPressed: () => answer(ApprovalDecision.always),
                child: const Text('Always'),
              ),
            ),
          ],
          const SizedBox(width: 8),
          Expanded(
            child: OutlinedButton(
              onPressed: () => answer(ApprovalDecision.deny),
              child: const Text('Deny'),
            ),
          ),
        ]),
      ]),
    );
  }
}
