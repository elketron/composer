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
    Widget action(String label, Color color, VoidCallback onTap, {Key? key, FontWeight weight = FontWeight.w400}) => Expanded(
          child: InkWell(
            key: key,
            onTap: onTap,
            child: Container(
              height: 44,
              alignment: Alignment.center,
              child: Text(label, style: TextStyle(fontSize: 13, color: color, fontWeight: weight)),
            ),
          ),
        );
    const divider = VerticalDivider(width: 1, thickness: 1, color: t.border);
    return Container(
      clipBehavior: Clip.antiAlias,
      decoration: BoxDecoration(
        color: t.raised,
        border: Border.all(color: t.warn),
        borderRadius: BorderRadius.circular(t.radiusCard),
      ),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(10, 8, 10, 8),
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Text(
              'APPROVAL · #${approval.session}${serverName == null ? '' : ' on $serverName'}',
              style: const TextStyle(fontSize: 11, color: t.warn, fontWeight: FontWeight.w600),
            ),
            const SizedBox(height: 2),
            Text.rich(TextSpan(children: [
              TextSpan(text: '${approvalQuestion(approval)} ', style: const TextStyle(color: t.muted, fontSize: 13)),
              TextSpan(text: command, style: mono(size: 13, color: t.text)),
            ])),
            if (approval.body.trim().isNotEmpty)
              Container(
                margin: const EdgeInsets.only(top: 8),
                padding: const EdgeInsets.all(8),
                constraints: const BoxConstraints(maxHeight: 200),
                decoration: BoxDecoration(color: t.bg, border: Border.all(color: t.border), borderRadius: BorderRadius.circular(t.radiusControl)),
                child: SingleChildScrollView(child: SelectableText(approval.body, style: mono(size: 12, color: t.text))),
              ),
            const SizedBox(height: 6),
            Text(approvalNote(approval), style: const TextStyle(color: t.faint, fontSize: 12)),
          ]),
        ),
        const Divider(height: 1),
        IntrinsicHeight(
          child: Row(children: [
            action('approve', t.ok, () => answer(ApprovalDecision.approve), key: Key('approve-${approval.request}'), weight: FontWeight.w600),
            if (approvalCanAlways(approval)) ...[divider, action('always', t.text, () => answer(ApprovalDecision.always), key: Key('always-${approval.request}'))],
            divider,
            action('deny', t.err, () => answer(ApprovalDecision.deny), key: Key('deny-${approval.request}')),
          ]),
        ),
      ]),
    );
  }
}
