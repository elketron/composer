import 'package:flutter/widgets.dart';

/// Tracks which full page is on top of the navigator (sheets, menus and dialogs don't count), so a
/// screen can tell when another page covers it: its live view is not kept running underneath.
class PageCover extends NavigatorObserver {
  final List<Route<dynamic>> _pages = [];

  /// The topmost full page, or null before there is one.
  final top = ValueNotifier<Route<dynamic>?>(null);

  void _changed() => top.value = _pages.lastOrNull;

  @override
  void didPush(Route<dynamic> route, Route<dynamic>? previousRoute) {
    if (route is PageRoute) {
      _pages.add(route);
      _changed();
    }
  }

  @override
  void didPop(Route<dynamic> route, Route<dynamic>? previousRoute) {
    if (_pages.remove(route)) _changed();
  }

  @override
  void didRemove(Route<dynamic> route, Route<dynamic>? previousRoute) {
    if (_pages.remove(route)) _changed();
  }

  @override
  void didReplace({Route<dynamic>? newRoute, Route<dynamic>? oldRoute}) {
    final at = oldRoute == null ? -1 : _pages.indexOf(oldRoute);
    if (at >= 0) _pages.removeAt(at);
    if (newRoute is PageRoute) _pages.insert(at < 0 ? _pages.length : at, newRoute);
    _changed();
  }
}

/// Hands the [PageCover] to the widget tree.
class PageCoverScope extends InheritedWidget {
  const PageCoverScope({super.key, required this.cover, required super.child});

  final PageCover cover;

  /// Null in a tree without one (tests): nothing is ever covered.
  static PageCover? maybeOf(BuildContext context) => context.getInheritedWidgetOfExactType<PageCoverScope>()?.cover;

  @override
  bool updateShouldNotify(PageCoverScope oldWidget) => cover != oldWidget.cover;
}
