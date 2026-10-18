import 'package:flutter/material.dart';

import '../theme/composer_theme.dart';
import '../theme/tokens.dart' as t;
import 'add_server_page.dart';

/// No servers yet.
class WelcomePage extends StatelessWidget {
  const WelcomePage({super.key});

  @override
  Widget build(BuildContext context) => Scaffold(
        body: Center(
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 380),
            child: Padding(
              padding: const EdgeInsets.all(24),
              child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.start, children: [
                Text('composer', style: mono(size: 22, color: t.text, weight: FontWeight.w600)),
                const SizedBox(height: 10),
                const Text(
                  'Chat with your agents and browse their sites, on any of your composer servers.',
                  style: TextStyle(color: t.muted, height: 1.5),
                ),
                const SizedBox(height: 24),
                FilledButton(
                  key: const Key('add-first-server'),
                  onPressed: () => AddServerPage.open(context),
                  child: const Text('Add a server'),
                ),
              ]),
            ),
          ),
        ),
      );
}
