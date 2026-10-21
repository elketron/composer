import 'package:flutter/material.dart';

import '../../state/projects.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../store_scope.dart';
import 'project_overview_view.dart';

/// One project as its own screen (phones).
class ProjectScreen extends StatelessWidget {
  const ProjectScreen({super.key, required this.url, required this.folder});

  final String url;
  final String folder;

  static void open(BuildContext context, String url, String folder) =>
      Navigator.of(context).push(MaterialPageRoute<void>(builder: (_) => ProjectScreen(url: url, folder: folder)));

  @override
  Widget build(BuildContext context) {
    final status = StoreScope.of(context).state.statusOf(url);
    return Scaffold(
      appBar: AppBar(
        titleSpacing: 0,
        title: Text(projectName(folder), style: mono(size: 15, color: t.text, weight: FontWeight.w600)),
        bottom: const PreferredSize(preferredSize: Size.fromHeight(1), child: Divider()),
      ),
      body: ProjectOverviewView(url: url, status: status, folder: folder),
    );
  }
}
