import 'package:flutter/material.dart';

import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;

/// What landed while the view was scrolled up: tap to go back to the newest line. The chat view
/// puts it by the newest end of the transcript.
class NewContentPill extends StatelessWidget {
  const NewContentPill({super.key, required this.count, required this.onTap, this.up = false});

  /// Rows that arrived since the view left the bottom.
  final int count;
  final VoidCallback onTap;

  /// The newest line is above (newest-first chat): the arrow points up.
  final bool up;

  @override
  Widget build(BuildContext context) => Material(
        color: t.raised,
        borderRadius: BorderRadius.circular(t.radius),
        child: InkWell(
          borderRadius: BorderRadius.circular(t.radius),
          onTap: onTap,
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
            child: Text('${up ? '↑' : '↓'} $count new', style: mono(size: 12, color: t.accent)),
          ),
        ),
      );
}
