import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../../state/server_status.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../home/section_label.dart';
import '../store_scope.dart';
import 'project_row.dart';

/// Search over the projects, then the ones with something going on and the recent ones (or the
/// search's results). The phone's Projects tab and the desktop's project picker.
class ProjectListView extends StatefulWidget {
  const ProjectListView({super.key, required this.url, required this.status, required this.onPick, this.autofocus = false, this.selected});

  final String url;
  final ServerStatus status;
  final void Function(ProjectSummary project) onPick;
  final bool autofocus;

  /// A folder to mark (the current scope).
  final String? selected;

  /// Recent projects shown before searching.
  static const recent = 8;

  @override
  State<ProjectListView> createState() => _ProjectListViewState();
}

class _ProjectListViewState extends State<ProjectListView> {
  late final TextEditingController _query = TextEditingController(text: widget.status.projectSearch.query);

  @override
  void initState() {
    super.initState();
    // The counts change: the list is asked again whenever it is shown (after the frame: not while
    // the tree is building).
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      StoreScope.read(context).dispatch(ProjectSearchChanged(widget.url, _query.text));
      StoreScope.read(context).dispatch(CatalogRequested(widget.url));
    });
  }

  @override
  void dispose() {
    _query.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final status = widget.status;
    final search = status.projectSearch;
    final searching = search.query.trim().isNotEmpty;
    Widget group(List<ProjectSummary> projects) => Container(
          decoration: BoxDecoration(color: t.surface, border: Border.all(color: t.border), borderRadius: BorderRadius.circular(t.radius)),
          child: Column(children: [
            for (final (i, p) in projects.indexed) ...[
              if (i > 0) const Divider(height: 1),
              ProjectRow(project: p, selected: p.folder == widget.selected, onTap: () => widget.onPick(p)),
            ],
          ]),
        );

    final all = status.projects;
    final active = all.where((p) => p.running > 0 || p.waiting > 0).toList();
    final recent = all.where((p) => !active.contains(p)).take(ProjectListView.recent).toList();
    final more = all.length - active.length - recent.length;
    return ListView(padding: const EdgeInsets.fromLTRB(14, 10, 14, 24), children: [
      TextField(
        key: const Key('project-search'),
        controller: _query,
        autofocus: widget.autofocus,
        decoration: InputDecoration(
          hintText: 'Search ${all.length} projects',
          prefixIcon: const Icon(Icons.search, size: 18, color: t.faint),
        ),
        onChanged: (q) => StoreScope.read(context).dispatch(ProjectSearchChanged(widget.url, q)),
        onSubmitted: (_) {
          final hits = status.projectSearch.hits;
          if (hits != null && hits.isNotEmpty) widget.onPick(hits.first);
        },
      ),
      if (searching) ...[
        const SectionLabel('Found'),
        if (search.error != null) Text(search.error!, style: mono(size: 12, color: t.err)),
        if (search.hits == null) const Padding(padding: EdgeInsets.all(12), child: Center(child: CircularProgressIndicator(strokeWidth: 2))),
        if (search.hits != null && search.hits!.isEmpty) const Text('No project matches.', style: TextStyle(color: t.faint, fontSize: 13)),
        if (search.hits != null && search.hits!.isNotEmpty) group(search.hits!),
      ] else ...[
        if (active.isNotEmpty) ...[const SectionLabel('Active'), group(active)],
        if (recent.isNotEmpty) ...[const SectionLabel('Recent'), group(recent)],
        if (more > 0)
          Padding(
            padding: const EdgeInsets.only(top: 10),
            child: Text('$more more: search to find them.', style: const TextStyle(fontSize: 12, color: t.faint)),
          ),
        if (all.isEmpty) Text('No projects in ${status.info?.projectsDir ?? 'the projects folder'} yet.', style: mono(size: 12, color: t.faint)),
      ],
    ]);
  }
}
