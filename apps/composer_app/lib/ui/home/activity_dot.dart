import 'package:flutter/widgets.dart';

import '../../state/inbox.dart' show isBusy;
import '../../theme/tokens.dart' as t;

export '../../state/inbox.dart' show isBusy;

/// The colour of what a session is doing (`ServerStatus.activity`).
Color activityColor(String activity) => switch (activity) {
      'running' || 'waiting' || 'starting' || 'approval' => t.warn,
      'done' => t.ok,
      'failed' => t.err,
      'paused' || 'stuck' => t.warn,
      _ => t.faint,
    };


/// A small rounded square in that colour; busy ones get a soft halo.
class ActivityDot extends StatelessWidget {
  const ActivityDot(this.activity, {super.key, this.size = 7});

  final String activity;
  final double size;

  @override
  Widget build(BuildContext context) {
    final color = activityColor(activity);
    return Container(
      width: size,
      height: size,
      decoration: BoxDecoration(
        color: color,
        borderRadius: BorderRadius.circular(t.radiusDot),
        boxShadow: isBusy(activity) ? [BoxShadow(color: color.withValues(alpha: 0.18), spreadRadius: 3)] : null,
      ),
    );
  }
}
