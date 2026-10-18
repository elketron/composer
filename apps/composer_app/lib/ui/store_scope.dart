import 'package:flutter/widgets.dart';

import '../flux/store.dart';
import '../state/actions.dart';
import '../state/app_state.dart';
import '../state/effects.dart';

typedef AppStore = Store<AppState, AppAction, AppEffect>;

/// Hands the store to the widget tree; widgets that read it rebuild when the state changes.
class StoreScope extends InheritedNotifier<AppStore> {
  const StoreScope({super.key, required AppStore store, required super.child}) : super(notifier: store);

  static AppStore of(BuildContext context) => context.dependOnInheritedWidgetOfExactType<StoreScope>()!.notifier!;

  /// For callbacks: no rebuild dependency.
  static AppStore read(BuildContext context) => context.getInheritedWidgetOfExactType<StoreScope>()!.notifier!;
}
