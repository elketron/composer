import 'package:flutter/material.dart';

import 'flux/store.dart';
import 'servers/effect_runner.dart';
import 'state/actions.dart';
import 'state/app_state.dart';
import 'state/reduce.dart';
import 'ui/composer_app.dart';
import 'ui/store_scope.dart';

void main() {
  WidgetsFlutterBinding.ensureInitialized();
  final effects = EffectRunner();
  final AppStore store = Store(AppState.initial, reduce, effects.run);
  store.dispatch(const AppStarted());
  AppLifecycleListener(onResume: () => store.dispatch(const AppResumed()));
  runApp(ComposerApp(store: store));
}
