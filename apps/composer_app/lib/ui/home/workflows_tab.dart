import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../../state/server_status.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../store_scope.dart';
import 'section_label.dart';
import 'workflow_tile.dart';

/// The workspace's workflows (sites with an application and/or MCP servers), with the sessions in
/// each and a way to start a chat in one.
class WorkflowsTab extends StatefulWidget {
  const WorkflowsTab({super.key, required this.url, required this.status});

  final String url;
  final ServerStatus status;

  @override
  State<WorkflowsTab> createState() => _WorkflowsTabState();
}

class _WorkflowsTabState extends State<WorkflowsTab> {
  @override
  void initState() {
    super.initState();
    // A site may have become a workflow since the catalog was fetched: ask again when the tab shows.
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) StoreScope.read(context).dispatch(CatalogRequested(widget.url));
    });
  }

  @override
  Widget build(BuildContext context) {
    final workflows = widget.status.workflows;
    if (workflows.isEmpty) {
      return Center(
        child: Padding(
          key: const Key('workflows-empty'),
          padding: const EdgeInsets.all(32),
          child: Text.rich(
            textAlign: TextAlign.center,
            TextSpan(style: const TextStyle(color: t.faint, height: 1.5), children: [
              const TextSpan(text: 'No workflows yet. A site is a workflow when its '),
              TextSpan(text: 'index.md', style: mono(size: 13, color: t.muted)),
              const TextSpan(text: ' sets '),
              TextSpan(text: 'app:', style: mono(size: 13, color: t.muted)),
              const TextSpan(text: ' or '),
              TextSpan(text: 'mcp:', style: mono(size: 13, color: t.muted)),
              const TextSpan(text: '.'),
            ]),
          ),
        ),
      );
    }
    return ListView(
      key: const PageStorageKey('workflows'),
      padding: const EdgeInsets.fromLTRB(14, 0, 14, 24),
      children: [
        SectionLabel('Workflows', trailing: Text('${workflows.length}', style: mono(size: 11, color: t.faint))),
        for (final site in workflows)
          Padding(
            padding: const EdgeInsets.only(bottom: 8),
            child: WorkflowTile(url: widget.url, status: widget.status, site: site),
          ),
      ],
    );
  }
}
