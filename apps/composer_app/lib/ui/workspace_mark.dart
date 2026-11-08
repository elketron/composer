import 'package:flutter/material.dart';

import '../state/workspace.dart';
import '../theme/composer_theme.dart';
import '../theme/tokens.dart' as t;

/// A workspace's colour dot, with its name: the same pair everywhere, so you always know which
/// workspace you are looking at.
class WorkspaceMark extends StatelessWidget {
  const WorkspaceMark({
    super.key,
    required this.workspace,
    this.showName = true,
    this.dotSize = 9,
    this.style,
  });

  final Workspace workspace;
  final bool showName;
  final double dotSize;
  final TextStyle? style;

  @override
  Widget build(BuildContext context) => Row(mainAxisSize: MainAxisSize.min, children: [
        Container(
          width: dotSize,
          height: dotSize,
          decoration: BoxDecoration(color: workspace.color.color, shape: BoxShape.circle),
        ),
        if (showName) ...[
          const SizedBox(width: 6),
          Flexible(
            child: Text(
              workspace.name,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: style ?? mono(size: 12.5, color: t.text, weight: FontWeight.w600),
            ),
          ),
        ],
      ]);
}

/// The workspace's colour as a thin line, for the top of a screen that has no room for the name.
class WorkspaceStrip extends StatelessWidget {
  const WorkspaceStrip({super.key, required this.workspace, this.height = 2});

  final Workspace workspace;
  final double height;

  @override
  Widget build(BuildContext context) => Container(height: height, color: workspace.color.color);
}
