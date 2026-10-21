import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';

import '../../theme/tokens.dart' as t;

/// A work's stages as dots: done, review, running, waiting.
class StageDots extends StatelessWidget {
  const StageDots(this.stages, {super.key});

  final List<StageState> stages;

  static Color colorOf(String status) => switch (status) {
        'done' || 'approved' => t.ok,
        'review' => t.accent,
        'running' || 'rerun' => t.warn,
        'stopped' || 'failed' => t.err,
        _ => t.borderStrong,
      };

  @override
  Widget build(BuildContext context) => Row(mainAxisSize: MainAxisSize.min, children: [
        for (final s in stages)
          Tooltip(
            message: '${s.name.replaceFirst('_', ' ')} · ${s.status}',
            child: Container(
              width: 7,
              height: 7,
              margin: const EdgeInsets.only(right: 3),
              decoration: BoxDecoration(
                shape: BoxShape.circle,
                color: s.status == 'waiting' ? null : colorOf(s.status),
                border: Border.all(color: colorOf(s.status)),
              ),
            ),
          ),
      ]);
}
