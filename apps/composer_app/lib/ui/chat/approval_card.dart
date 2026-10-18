import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../store_scope.dart';

/// A recipe waiting for your answer: what runs, and Approve / Deny.
class ApprovalCard extends StatelessWidget {
  const ApprovalCard({super.key, required this.url, required this.approval, this.serverName});

  final String url;
  final ServerMessageApprovalRequest approval;

  /// Shown when approvals from several servers are listed together.
  final String? serverName;

  @override
  Widget build(BuildContext context) {
    final store = StoreScope.read(context);
    final command = [approval.site, '›', approval.recipe, ...approval.args].join(' ');
    return Container(
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: t.surface,
        border: Border.all(color: t.warn),
        borderRadius: BorderRadius.circular(t.radius),
      ),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        Text.rich(TextSpan(children: [
          const TextSpan(text: 'Run recipe? ', style: TextStyle(color: t.warn, fontSize: 14)),
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
        const Text(
          'Once approved, this recipe runs without asking until it changes.',
          style: TextStyle(color: t.faint, fontSize: 12),
        ),
        const SizedBox(height: 10),
        Row(children: [
          Expanded(
            child: FilledButton(
              key: Key('approve-${approval.request}'),
              onPressed: () => store.dispatch(ApprovalAnswered(url, approval.request, approve: true)),
              child: const Text('Approve'),
            ),
          ),
          const SizedBox(width: 8),
          Expanded(
            child: OutlinedButton(
              onPressed: () => store.dispatch(ApprovalAnswered(url, approval.request, approve: false)),
              child: const Text('Deny'),
            ),
          ),
        ]),
      ]),
    );
  }
}
