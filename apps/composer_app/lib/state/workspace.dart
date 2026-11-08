import 'package:composer_protocol/composer_protocol.dart';

import 'server_entry.dart';
import 'workspace_color.dart';

/// A workspace as the phone shows it: its name and colour.
///
/// The server reports both in `/api/info`; a server from before workspaces leaves them out, so the
/// name comes from the URL (`/w/<name>`) and the colour from the name — the same derivation the
/// server would have used.
class Workspace {
  const Workspace(this.name, this.color);

  final String name;
  final WorkspaceColor color;

  factory Workspace.of(String url, {ServerInfo? info, ServerEntry? entry}) {
    final name = _firstNonEmpty([info?.workspace, entry?.workspace, ServerEntry.workspaceIn(url)]);
    final hex = _firstNonEmpty([info?.workspaceColor, entry?.color]);
    return Workspace(name, WorkspaceColor.parse(hex) ?? WorkspaceColor.fromName(name));
  }

  static String _firstNonEmpty(List<String?> values) => values
      .map((v) => v?.trim() ?? '')
      .firstWhere((v) => v.isNotEmpty, orElse: () => 'default');
}
