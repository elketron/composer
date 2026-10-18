import 'package:flutter/material.dart';

import '../theme/tokens.dart' as t;
import 'chat/approval_card.dart';
import 'store_scope.dart';

/// Shown on every screen while any server has a recipe waiting for you; opens the list.
class ApprovalBar extends StatelessWidget {
  const ApprovalBar({super.key});

  @override
  Widget build(BuildContext context) {
    final state = StoreScope.of(context).state;
    final count = state.servers.fold(0, (n, s) => n + state.statusOf(s.url).approvals.length);
    if (count == 0) return const SizedBox.shrink();
    return Material(
      color: t.warn.withValues(alpha: 0.12),
      child: InkWell(
        key: const Key('approval-bar'),
        onTap: () => _open(context),
        child: Container(
          height: 36,
          padding: const EdgeInsets.symmetric(horizontal: 16),
          decoration: const BoxDecoration(border: Border(bottom: BorderSide(color: t.warn))),
          child: Row(children: [
            const Icon(Icons.gpp_maybe_outlined, size: 16, color: t.warn),
            const SizedBox(width: 8),
            Text(
              count == 1 ? 'A recipe needs your approval' : '$count recipes need your approval',
              style: const TextStyle(color: t.warn, fontSize: 13),
            ),
            const Spacer(),
            const Text('Review', style: TextStyle(color: t.warn, fontSize: 13, fontWeight: FontWeight.w600)),
          ]),
        ),
      ),
    );
  }

  void _open(BuildContext context) => showModalBottomSheet<void>(
        context: context,
        isScrollControlled: true,
        showDragHandle: true,
        constraints: const BoxConstraints(maxWidth: 720),
        builder: (sheet) => ListenableBuilder(
          listenable: StoreScope.read(context),
          builder: (_, _) {
            final state = StoreScope.read(context).state;
            final several = state.servers.length > 1;
            final cards = [
              for (final server in state.servers)
                for (final approval in state.statusOf(server.url).approvals)
                  ApprovalCard(url: server.url, approval: approval, serverName: several ? server.name : null),
            ];
            if (cards.isEmpty) WidgetsBinding.instance.addPostFrameCallback((_) => Navigator.of(sheet).maybePop());
            return SafeArea(
              child: ConstrainedBox(
                constraints: BoxConstraints(maxHeight: MediaQuery.sizeOf(context).height * 0.8),
                child: ListView.separated(
                  shrinkWrap: true,
                  padding: const EdgeInsets.fromLTRB(16, 0, 16, 16),
                  itemCount: cards.length,
                  separatorBuilder: (_, _) => const SizedBox(height: 12),
                  itemBuilder: (_, i) => cards[i],
                ),
              ),
            );
          },
        ),
      );
}
