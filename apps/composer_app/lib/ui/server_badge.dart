import 'package:flutter/material.dart';

import '../state/server_mark.dart';
import '../theme/composer_theme.dart';
import '../theme/tokens.dart' as t;

/// A server's colour dot, with its name.
class ServerBadge extends StatelessWidget {
  const ServerBadge({
    super.key,
    required this.mark,
    this.showName = true,
    this.dotSize = 9,
    this.style,
  });

  final ServerMark mark;
  final bool showName;
  final double dotSize;
  final TextStyle? style;

  @override
  Widget build(BuildContext context) => Row(mainAxisSize: MainAxisSize.min, children: [
        Container(
          width: dotSize,
          height: dotSize,
          decoration: BoxDecoration(color: mark.color.color, shape: BoxShape.circle),
        ),
        if (showName) ...[
          const SizedBox(width: 6),
          Flexible(
            child: Text(
              mark.name,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: style ?? mono(size: 12.5, color: t.text, weight: FontWeight.w600),
            ),
          ),
        ],
      ]);
}

/// The server's colour as a thin line, for the top of a screen that has no room for the name.
class ServerStrip extends StatelessWidget {
  const ServerStrip({super.key, required this.mark, this.height = 2});

  final ServerMark mark;
  final double height;

  @override
  Widget build(BuildContext context) => Container(height: height, color: mark.color.color);
}
