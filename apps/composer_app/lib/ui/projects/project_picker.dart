import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';

import '../store_scope.dart';
import 'project_list_view.dart';

/// A dialog to find a project; `onPick` gets the chosen one (wide screens).
void showProjectPicker(BuildContext context, String url, void Function(ProjectSummary project) onPick, {String? selected}) =>
    showDialog<void>(
      context: context,
      builder: (dialog) => Dialog(
        alignment: const Alignment(0, -0.6),
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 520, maxHeight: 560),
          child: ProjectPicker(url: url, selected: selected, onPick: (p) {
            Navigator.pop(dialog);
            onPick(p);
          }),
        ),
      ),
    );

/// The picker's content: the project list with its search, kept up to date.
class ProjectPicker extends StatelessWidget {
  const ProjectPicker({super.key, required this.url, required this.onPick, this.selected});

  final String url;
  final void Function(ProjectSummary project) onPick;
  final String? selected;

  @override
  Widget build(BuildContext context) {
    final status = StoreScope.of(context).state.statusOf(url);
    return ProjectListView(url: url, status: status, onPick: onPick, autofocus: true, selected: selected);
  }
}
