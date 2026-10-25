import 'package:flutter/widgets.dart';

import '../../theme/tokens.dart' as t;

/// The colour of what a session is doing (`ServerStatus.activity`).
Color activityColor(String activity) => switch (activity) {
      'running' || 'waiting' || 'starting' || 'approval' => t.warn,
      'done' => t.ok,
      'failed' => t.err,
      'paused' || 'stuck' => t.warn,
      _ => t.faint,
    };

bool isBusy(String activity) => const {'running', 'waiting', 'starting', 'approval'}.contains(activity);

/// A round dot in that colour; busy ones get a soft halo.
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
        shape: BoxShape.circle,
        boxShadow: isBusy(activity) ? [BoxShadow(color: color.withValues(alpha: 0.18), spreadRadius: 3)] : null,
      ),
    );
  }
}
