import 'package:flutter/material.dart';

import '../../state/workspace.dart';
import '../store_scope.dart';
import '../workspace_mark.dart';
import 'reader.dart';

/// The reader as its own screen (phones).
class ReaderScreen extends StatelessWidget {
  const ReaderScreen({super.key, required this.url});

  final String url;

  @override
  Widget build(BuildContext context) {
    final store = StoreScope.of(context);
    final status = store.state.statusOf(url);
    final workspace = Workspace.of(url, info: status.info, entry: store.state.serverFor(url));
    return Scaffold(
      body: SafeArea(
        child: Column(children: [
          WorkspaceStrip(workspace: workspace),
          Expanded(child: Reader(url: url, status: status, onClose: () => Navigator.of(context).maybePop())),
        ]),
      ),
    );
  }
}
