import 'package:flutter/material.dart';

import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;

/// A quiet one-line row (tool call, thinking, compaction) that expands to show its body.
class CollapsibleRow extends StatefulWidget {
  const CollapsibleRow({super.key, required this.label, this.body, this.trailing});

  final InlineSpan label;

  /// Shown when expanded; null makes the row static.
  final String? body;
  final InlineSpan? trailing;

  @override
  State<CollapsibleRow> createState() => _CollapsibleRowState();
}

class _CollapsibleRowState extends State<CollapsibleRow> {
  bool _open = false;

  @override
  Widget build(BuildContext context) {
    final body = widget.body;
    final expandable = body != null && body.trim().isNotEmpty;
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      InkWell(
        borderRadius: BorderRadius.circular(t.radiusSm),
        onTap: expandable ? () => setState(() => _open = !_open) : null,
        child: Padding(
          padding: const EdgeInsets.symmetric(vertical: 2),
          child: Text.rich(
            TextSpan(style: mono(size: 12, color: t.faint), children: [
              TextSpan(text: expandable && _open ? '▾ ' : '▸ '),
              widget.label,
              if (widget.trailing != null) ...[const TextSpan(text: ' '), widget.trailing!],
            ]),
            maxLines: _open ? null : 1,
            overflow: _open ? null : TextOverflow.ellipsis,
          ),
        ),
      ),
      if (expandable && _open)
        Container(
          margin: const EdgeInsets.only(top: 4, bottom: 4),
          padding: const EdgeInsets.all(8),
          constraints: const BoxConstraints(maxHeight: 320),
          decoration: BoxDecoration(
            color: t.bg,
            border: Border.all(color: t.border),
            borderRadius: BorderRadius.circular(t.radius),
          ),
          child: SingleChildScrollView(child: SelectableText(body, style: mono(size: 12, color: t.muted))),
        ),
    ]);
  }
}
