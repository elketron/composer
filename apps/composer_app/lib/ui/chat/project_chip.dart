import 'package:flutter/material.dart';

import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../../state/actions.dart';
import '../../state/types.dart';
import '../store_scope.dart';
import 'project_sheet.dart';

/// The open chat's project in its header: a click opens the project beside the chat (none yet:
/// picks one). ⋮ changes or unsets it.
class ProjectChip extends StatelessWidget {
  const ProjectChip({super.key, required this.url, required this.project});

  final String url;

  /// The folder; null: none.
  final String? project;

  @override
  Widget build(BuildContext context) => Tooltip(
        message: project == null ? 'No project: click to set one' : '$project: click to see the project',
        child: InkWell(
          key: const Key('project-chip'),
          onTap: () => project == null
              ? showProjectSheet(context, url)
              : StoreScope.read(context).dispatch(SideTabSelected(url, SideTab.project, project: project)),
          borderRadius: BorderRadius.circular(t.radiusSm),
          child: Container(
            padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 1),
            decoration: BoxDecoration(border: Border.all(color: t.borderStrong), borderRadius: BorderRadius.circular(t.radiusSm)),
            child: Text(project?.split('/').last ?? 'no project', style: mono(size: 11, color: project == null ? t.faint : t.muted)),
          ),
        ),
      );
}
