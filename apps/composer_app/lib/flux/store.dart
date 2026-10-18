import 'package:flutter/foundation.dart';

/// Flux, as in the TUI: actions go through a pure reducer that returns the next state plus
/// effects (IO), which run outside it and dispatch further actions. Widgets only dispatch and read.
class Store<S, A, E> extends ChangeNotifier {
  Store(this._state, this._reduce, this._run);

  S _state;
  final (S, List<E>) Function(S, A) _reduce;
  final void Function(E effect, void Function(A) dispatch) _run;

  /// Actions dispatched while one is being handled (by an effect, synchronously) wait their turn.
  final List<A> _queue = [];
  bool _busy = false;

  S get state => _state;

  void dispatch(A action) {
    _queue.add(action);
    if (_busy) return;
    _busy = true;
    try {
      while (_queue.isNotEmpty) {
        final (next, effects) = _reduce(_state, _queue.removeAt(0));
        final changed = !identical(next, _state);
        _state = next;
        if (changed) notifyListeners();
        for (final effect in effects) {
          _run(effect, dispatch);
        }
      }
    } finally {
      _busy = false;
    }
  }
}
