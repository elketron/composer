import 'package:flutter/material.dart';

import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;

/// A session's workflow as a small label (next to its project in lists).
class WorkflowTag extends StatelessWidget {
  const WorkflowTag(this.name, {super.key});

  final String name;

  @override
  Widget build(BuildContext context) => Container(
        padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 1),
        decoration: BoxDecoration(
          color: t.accent.withValues(alpha: 0.10),
          border: Border.all(color: t.accent.withValues(alpha: 0.6)),
          borderRadius: BorderRadius.circular(t.radiusSm),
        ),
        child: Text(name, style: mono(size: 11, color: t.accent)),
      );
}
