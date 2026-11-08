import 'package:flutter/material.dart';

import 'add_server_page.dart';

/// "Add server" below the list.
class ServerListFooter extends StatelessWidget {
  const ServerListFooter({super.key});

  @override
  Widget build(BuildContext context) => Padding(
        padding: const EdgeInsets.fromLTRB(8, 0, 8, 10),
        child: OutlinedButton.icon(
          key: const Key('add-workspace'),
          onPressed: () => AddServerPage.open(context),
          icon: const Icon(Icons.add, size: 16),
          label: const Text('Add a workspace'),
        ),
      );
}
