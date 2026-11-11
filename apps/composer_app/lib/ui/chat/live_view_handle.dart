import 'package:flutter/material.dart';

import '../../state/live_view_size.dart';
import '../../theme/tokens.dart' as t;

/// The bar under the live view: the application's name with its live dot at the left, the grip in
/// the middle, "view only" at the right. Drag it to grow or shrink the view, tap it to step to the
/// next size (screen readers hear the size as the value and can increase or decrease it).
/// Square, edge to edge, 44px high.
class LiveViewHandle extends StatelessWidget {
  const LiveViewHandle({
    super.key,
    required this.app,
    required this.live,
    required this.size,
    required this.onTap,
    required this.onDragStart,
    required this.onDragUpdate,
    required this.onDragEnd,
    this.onGrow,
    this.onShrink,
  });

  /// The application's name.
  final String app;

  /// The page loaded (the dot is on); false when it could not.
  final bool live;
  final LiveViewSize size;
  final VoidCallback onTap;
  final VoidCallback onDragStart;

  /// The finger moved by this much (down is positive).
  final void Function(double dy) onDragUpdate;
  final VoidCallback onDragEnd;

  /// Screen-reader increase / decrease.
  final VoidCallback? onGrow;
  final VoidCallback? onShrink;

  @override
  Widget build(BuildContext context) {
    const label = TextStyle(fontSize: 11, color: t.muted);
    return Semantics(
      button: true,
      label: 'Resize the live view',
      hint: '$app, view only',
      value: size.name,
      increasedValue: onGrow == null ? null : LiveViewSize.values[size.index + 1].name,
      decreasedValue: onShrink == null ? null : LiveViewSize.values[size.index - 1].name,
      onIncrease: onGrow,
      onDecrease: onShrink,
      excludeSemantics: true,
      child: GestureDetector(
        key: const Key('live-view-handle'),
        behavior: HitTestBehavior.opaque,
        onTap: onTap,
        onVerticalDragStart: (_) => onDragStart(),
        onVerticalDragUpdate: (d) => onDragUpdate(d.delta.dy),
        onVerticalDragEnd: (_) => onDragEnd(),
        onVerticalDragCancel: onDragEnd,
        child: Container(
          height: 44,
          padding: const EdgeInsets.symmetric(horizontal: 12),
          decoration: const BoxDecoration(color: t.surface, border: Border.symmetric(horizontal: BorderSide(color: t.border))),
          child: Row(children: [
            Expanded(
              child: Row(children: [
                Container(
                  key: const Key('live-view-dot'),
                  width: 6,
                  height: 6,
                  decoration: BoxDecoration(color: live ? t.err : t.faint, borderRadius: BorderRadius.circular(t.radiusDot)),
                ),
                const SizedBox(width: 6),
                Flexible(child: Text('$app · live', key: const Key('live-view-app'), maxLines: 1, overflow: TextOverflow.ellipsis, style: label.copyWith(color: t.text))),
              ]),
            ),
            const SizedBox(width: 10),
            const SizedBox(key: Key('live-view-grip'), width: 36, height: 3, child: ColoredBox(color: t.borderStrong)),
            const SizedBox(width: 10),
            const Expanded(child: Text('view only', key: Key('live-view-only'), maxLines: 1, textAlign: TextAlign.end, style: label)),
          ]),
        ),
      ),
    );
  }
}
