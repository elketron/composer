import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../store_scope.dart';

/// Opens the workflow picker for the open chat.
void showWorkflowSheet(BuildContext context, String url) {
  // The workspace's sites may have changed since the catalog was fetched.
  StoreScope.read(context).dispatch(CatalogRequested(url));
  showModalBottomSheet<void>(
    context: context,
    isScrollControlled: true,
    showDragHandle: true,
    builder: (_) => WorkflowSheet(url: url),
  );
}

/// The workspace's workflows, the current one marked, and a way to leave it. Choosing one sends
/// `site.set` for the open chat; the reply's sentence comes back as a notice.
class WorkflowSheet extends StatelessWidget {
  const WorkflowSheet({super.key, required this.url});

  final String url;

  @override
  Widget build(BuildContext context) {
    final store = StoreScope.of(context);
    final status = store.state.statusOf(url);
    final chat = status.chat;
    final current = chat == null ? null : status.session(chat.session)?.site;
    final workflows = status.workflows;

    void choose(String? site) {
      Navigator.pop(context);
      if (site != current) store.dispatch(SiteChosen(url, site));
    }

    Widget row(String name, {required String title, String? detail, required bool on, Key? key, Color? color}) => InkWell(
          key: key,
          onTap: () => choose(name.isEmpty ? null : name),
          child: Container(
            constraints: const BoxConstraints(minHeight: 52),
            padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 8),
            decoration: const BoxDecoration(border: Border(top: BorderSide(color: t.border))),
            child: Row(children: [
              SizedBox(width: 22, child: on ? const Icon(Icons.check, size: 18, color: t.accent) : null),
              Expanded(
                child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                  Text.rich(TextSpan(children: [
                    TextSpan(text: name.isEmpty ? title : name, style: mono(size: 13.5, color: color ?? (on ? t.accent : t.text), weight: FontWeight.w600)),
                    if (name.isNotEmpty && title.isNotEmpty && title != name) TextSpan(text: '  $title', style: const TextStyle(fontSize: 12.5, color: t.muted)),
                  ])),
                  if (detail != null) Text(detail, style: mono(size: 11, color: t.faint)),
                ]),
              ),
            ]),
          ),
        );

    return SafeArea(
      child: ConstrainedBox(
        constraints: BoxConstraints(maxHeight: MediaQuery.sizeOf(context).height * 0.7),
        child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(14, 0, 14, 10),
            child: Text('Workflow', style: mono(size: 13, color: t.faint, weight: FontWeight.w600)),
          ),
          Flexible(
            child: ListView(shrinkWrap: true, padding: EdgeInsets.zero, children: [
              if (workflows.isEmpty)
                Padding(
                  key: const Key('workflow-sheet-empty'),
                  padding: const EdgeInsets.fromLTRB(14, 8, 14, 16),
                  child: Text(
                    'No workflows in this workspace. A site is a workflow when its index.md sets app: or mcp:.',
                    style: const TextStyle(fontSize: 13, color: t.faint),
                  ),
                ),
              for (final site in workflows)
                row(
                  site.name,
                  title: site.title,
                  detail: [
                    if ((site.app ?? '').isNotEmpty) 'app ${site.app}',
                    if (site.mcp.isNotEmpty) 'mcp ${site.mcp.join(', ')}',
                  ].join(' · '),
                  on: site.name == current,
                  key: Key('workflow-pick-${site.name}'),
                ),
              if (current != null)
                row('', title: 'Leave ${status.workflows.any((w) => w.name == current) ? 'the workflow' : current}', on: false, key: const Key('workflow-leave'), color: t.err),
            ]),
          ),
        ]),
      ),
    );
  }
}
