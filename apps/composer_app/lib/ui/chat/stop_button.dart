import 'package:flutter/material.dart';

import '../../theme/tokens.dart' as t;

/// The header's stop control while a turn runs: a filled rounded square in the error colour on a
/// 44x44 button.
class StopButton extends StatelessWidget {
  const StopButton({super.key, required this.onPressed});

  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) => Tooltip(
        message: 'Stop the agent',
        child: Semantics(
          button: true,
          label: 'Stop the agent',
          excludeSemantics: true,
          child: Material(
            color: t.raised,
            shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(t.radiusControl), side: const BorderSide(color: t.borderStrong)),
            child: InkWell(
              borderRadius: BorderRadius.circular(t.radiusControl),
              onTap: onPressed,
              child: SizedBox(
                width: 44,
                height: 44,
                child: Center(
                  child: Container(width: 14, height: 14, decoration: BoxDecoration(color: t.err, borderRadius: BorderRadius.circular(t.radiusDot))),
                ),
              ),
            ),
          ),
        ),
      );
}
