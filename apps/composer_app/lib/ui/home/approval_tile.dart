import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../store_scope.dart';

/// A recipe waiting for you, answered in place; the recipe itself is one tap away.
class ApprovalTile extends StatelessWidget {
  const ApprovalTile({super.key, required this.url, required this.approval, this.agent});

  final String url;
  final ServerMessageApprovalRequest approval;

  /// The asking session's agent.
  final String? agent;

  @override
  Widget build(BuildContext context) {
    final store = StoreScope.read(context);
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
          const TextSpan(text: ' wants to run'),
        ])),
        const SizedBox(height: 4),
        Text([approval.site, '›', approval.recipe, ...approval.args].join(' '), style: mono(size: 13, color: t.text)),
        const SizedBox(height: 8),
        Row(children: [
          FilledButton(
            key: Key('approve-${approval.request}'),
            style: FilledButton.styleFrom(visualDensity: VisualDensity.compact),
            onPressed: () => store.dispatch(ApprovalAnswered(url, approval.request, approve: true)),
            child: const Text('Approve'),
          ),
          const SizedBox(width: 8),
          OutlinedButton(
            style: OutlinedButton.styleFrom(visualDensity: VisualDensity.compact),
            onPressed: () => store.dispatch(ApprovalAnswered(url, approval.request, approve: false)),
            child: const Text('Deny'),
          ),
          const SizedBox(width: 4),
          TextButton(
            style: TextButton.styleFrom(foregroundColor: t.muted, visualDensity: VisualDensity.compact),
            onPressed: () => _showRecipe(context),
            child: const Text('View recipe'),
          ),
        ]),
      ]),
    );
  }

  void _showRecipe(BuildContext context) => showModalBottomSheet<void>(
        context: context,
        showDragHandle: true,
        isScrollControlled: true,
        builder: (_) => SafeArea(
          child: Padding(
            padding: const EdgeInsets.fromLTRB(16, 0, 16, 16),
            child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: [
              Text('${approval.site} › ${approval.recipe}', style: mono(size: 14, color: t.text)),
              const SizedBox(height: 10),
              Container(
                constraints: const BoxConstraints(maxHeight: 360),
                padding: const EdgeInsets.all(10),
                decoration: BoxDecoration(color: t.bg, border: Border.all(color: t.border), borderRadius: BorderRadius.circular(t.radius)),
                child: SingleChildScrollView(child: SelectableText(approval.body, style: mono(size: 12, color: t.text))),
              ),
              const SizedBox(height: 8),
              const Text('Once approved, this recipe runs without asking until it changes.', style: TextStyle(color: t.faint, fontSize: 12)),
            ]),
          ),
        ),
      );
}
