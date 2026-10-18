import 'package:flutter/material.dart';

import '../../theme/tokens.dart' as t;

/// A 42px icon button with a filled background.
class SquareButton extends StatelessWidget {
  const SquareButton({super.key, required this.tooltip, required this.color, required this.child, this.onPressed});

  final String tooltip;
  final Color color;
  final Widget child;
  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) => Tooltip(
        message: tooltip,
        child: Material(
          color: onPressed == null ? color.withValues(alpha: 0.4) : color,
          borderRadius: BorderRadius.circular(t.radius),
          child: InkWell(
            borderRadius: BorderRadius.circular(t.radius),
            onTap: onPressed,
            child: SizedBox(width: 42, height: 42, child: Center(child: child)),
          ),
        ),
      );
}
