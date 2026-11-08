import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';

/// A session's desk as the phone shows it: the tabs that make sense without the desktop's
/// terminal pane (apps, pages, changes), their labels, and which one is shown.
class DeskView {
  const DeskView(this.desk);

  final Desk desk;

  /// Terminals stay on the server for the desktop; the phone lists the rest.
  List<DeskEntry> get tabs =>
      [for (final entry in desk.tabs) if (entry.tab is! DeskTabTerminal) entry];

  /// The desk's active tab, else the first shown one.
  DeskEntry? get shown {
    final visible = tabs;
    if (visible.isEmpty) return null;
    return visible.where((e) => e.id == desk.active).firstOrNull ?? visible.first;
  }

  /// Tabs opened by the agent and not looked at yet.
  int get unseen => tabs.where((t) => t.unseen).length;

  DeskEntry? byId(String id) => tabs.where((e) => e.id == id).firstOrNull;

  static String label(DeskEntry entry) => switch (entry.tab) {
        DeskTabApp(:final site, :final project) => project == null ? site : '$site · $project',
        DeskTabPage(:final url) =>
          url.replaceFirst('site://', '').replaceFirst(RegExp(r'\.md$'), ''),
        DeskTabChanges(:final project) => 'changes · $project',
        DeskTabTerminal() => 'terminal',
        _ => 'tab',
      };

  static IconData icon(DeskEntry entry) => switch (entry.tab) {
        DeskTabApp() => Icons.grid_view_outlined,
        DeskTabPage() => Icons.description_outlined,
        DeskTabChanges() => Icons.difference_outlined,
        DeskTabTerminal() => Icons.terminal,
        _ => Icons.tab,
      };
}
