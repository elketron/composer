import 'package:flutter/material.dart';

import '../../theme/tokens.dart' as t;

/// The live view folded beside a full-width chat (landscape): 52px wide, the application's name
/// on its side with the live dot, and a chevron. A tap unfolds it.
class LiveViewRail extends StatelessWidget {
  const LiveViewRail({super.key, required this.app, required this.onTap});

  final String app;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) => Semantics(
        button: true,
        label: 'Show the live view of $app',
        excludeSemantics: true,
        child: Material(
          color: t.surface,
          child: InkWell(
            onTap: onTap,
            child: Container(
              width: 52,
              decoration: const BoxDecoration(border: Border(left: BorderSide(color: t.border))),
              child: Column(mainAxisAlignment: MainAxisAlignment.center, children: [
                Container(width: 6, height: 6, decoration: BoxDecoration(color: t.err, borderRadius: BorderRadius.circular(t.radiusDot))),
                const SizedBox(height: 10),
                RotatedBox(quarterTurns: 3, child: Text('$app · live', maxLines: 1, style: const TextStyle(fontSize: 11, color: t.text))),
                const SizedBox(height: 10),
                const Icon(Icons.chevron_left, size: 18, color: t.muted),
              ]),
            ),
          ),
        ),
      );
}
