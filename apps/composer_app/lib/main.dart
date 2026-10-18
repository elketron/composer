import 'package:flutter/material.dart';

import 'flux/store.dart';
import 'servers/effect_runner.dart';
import 'servers/notifier.dart';
import 'state/actions.dart';
import 'state/app_state.dart';
import 'state/reduce.dart';
import 'ui/composer_app.dart';
import 'ui/store_scope.dart';

void main() {
  WidgetsFlutterBinding.ensureInitialized();
  final notifier = Notifier();
  final effects = EffectRunner(notifier);
  final AppStore store = Store(AppState.initial, reduce, effects.run);
  store.dispatch(const AppStarted());
  // In front and focused, or not: decides what gets a notification.
  AppLifecycleListener(onStateChange: (state) => store.dispatch(ForegroundChanged(state == AppLifecycleState.resumed)));
  runApp(ComposerApp(store: store, notifier: notifier));
  notifier.start();
}
