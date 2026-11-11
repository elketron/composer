import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';

import '../../state/server_status.dart';
import '../../theme/tokens.dart' as t;
import 'workflow_sheet.dart';

/// Under the header: the session's workflow as a chip (tap: the workflow sheet), its project, and
/// its model at the right.
class WorkflowRow extends StatelessWidget {
  const WorkflowRow({super.key, required this.url, required this.status, required this.session});

  final String url;
  final ServerStatus status;
  final SessionInfo? session;

  @override
  Widget build(BuildContext context) {
    final site = session?.site;
    final project = session?.project?.split('/').last;
    final model = session?.model ?? status.info?.model;
    final chipColor = site == null ? t.muted : t.accent;
    return Container(
      constraints: const BoxConstraints(minHeight: 52),
      padding: const EdgeInsets.symmetric(horizontal: 14),
      decoration: const BoxDecoration(color: t.bg, border: Border(bottom: BorderSide(color: t.border))),
      child: Row(children: [
        // The touch target is 44px; the chip drawn in it is 32.
        Semantics(
          button: true,
          label: site == null ? 'No workflow: choose one' : 'Workflow $site: change',
          excludeSemantics: true,
          child: InkWell(
            key: const Key('workflow-chip'),
            borderRadius: BorderRadius.circular(t.radiusControl),
            onTap: session == null ? null : () => showWorkflowSheet(context, url),
            child: ConstrainedBox(
              constraints: const BoxConstraints(minHeight: 44, maxWidth: 190),
              child: Center(
                widthFactor: 1,
                // Hugs its text: a Container with an alignment would fill the width.
                child: Container(
                  height: 32,
                  padding: const EdgeInsets.symmetric(horizontal: 10),
                  decoration: BoxDecoration(
                    color: site == null ? Colors.transparent : t.accent.withValues(alpha: 0.12),
                    border: Border.all(color: site == null ? t.borderStrong : t.accent),
                    borderRadius: BorderRadius.circular(t.radiusControl),
                  ),
                  child: Center(
                    widthFactor: 1,
                    child: Text(
                      site ?? 'no workflow',
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(fontSize: 12, color: chipColor, fontWeight: site == null ? FontWeight.w400 : FontWeight.w600),
                    ),
                  ),
                ),
              ),
            ),
          ),
        ),
        const SizedBox(width: 8),
        Expanded(child: Text(project ?? 'no project', maxLines: 1, overflow: TextOverflow.ellipsis, style: const TextStyle(fontSize: 12, color: t.muted))),
        if (model != null) ...[
          const SizedBox(width: 8),
          ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 140),
            child: Text(model, key: const Key('session-model'), maxLines: 1, overflow: TextOverflow.ellipsis, style: const TextStyle(fontSize: 11, color: t.muted)),
          ),
        ],
      ]),
    );
  }
}
