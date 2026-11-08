import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../../theme/tokens.dart' as t;
import '../store_scope.dart';

/// What the user points at in an app: goes with the next message, or is dropped here.
class SelectionChip extends StatelessWidget {
  const SelectionChip({super.key, required this.url, required this.selection});

  final String url;
  final Selection selection;

  @override
  Widget build(BuildContext context) => Container(
        key: const Key('selection-chip'),
        margin: const EdgeInsets.fromLTRB(10, 6, 10, 0),
        padding: const EdgeInsets.fromLTRB(10, 7, 4, 7),
        decoration: BoxDecoration(
          color: t.accent.withValues(alpha: 0.10),
          border: Border.all(color: t.accent.withValues(alpha: 0.45)),
          borderRadius: BorderRadius.circular(t.radius),
        ),
        child: Row(children: [
          Icon(Icons.my_location, size: 15, color: t.accent),
          const SizedBox(width: 7),
          Expanded(
            child: Text(
              '${selection.label} · ${selection.app}',
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: const TextStyle(fontSize: 12.5, color: t.text),
            ),
          ),
          IconButton(
            key: const Key('selection-clear'),
            tooltip: 'Drop the selection',
            visualDensity: VisualDensity.compact,
            icon: const Icon(Icons.close, size: 16, color: t.muted),
            onPressed: () => StoreScope.read(context).dispatch(SelectionCleared(url)),
          ),
        ]),
      );
}
