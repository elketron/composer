import 'package:flutter/material.dart';

import 'home/phone_home.dart';
import 'store_scope.dart';
import 'welcome_page.dart';

/// The app frame: `PhoneHome` (tabs; the title opens the switcher) for the active server, or the
/// welcome page before there is one.
class Shell extends StatelessWidget {
  const Shell({super.key});

  @override
  Widget build(BuildContext context) {
    final state = StoreScope.of(context).state;
    if (!state.loaded) return const Scaffold();
    final server = state.activeServer;
    if (server == null) return const WelcomePage();
    return PhoneHome(key: ValueKey(server.url), server: server, status: state.statusOf(server.url));
  }
}
