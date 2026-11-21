import 'package:composer_protocol/composer_protocol.dart';

import 'server_entry.dart';
import 'workspace_color.dart';

/// A server's home as the phone shows it: its name and colour.
// rewrite: phase 2 — a server has one home now (no workspaces); every home is `default`, with the
// colour derived from that name, until the mark is reworked.
class Workspace {
  const Workspace(this.name, this.color);

  final String name;
  final WorkspaceColor color;

  factory Workspace.of(String url, {ServerInfo? info, ServerEntry? entry}) =>
      Workspace('default', WorkspaceColor.fromName('default'));
}
