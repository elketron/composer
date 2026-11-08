import 'server_entry.dart';

/// The paired workspaces of one host: a phone pairs a workspace at a time, so what it lists are
/// workspaces; they are grouped by the machine that serves them.
class WorkspaceGroup {
  const WorkspaceGroup(this.host, this.entries);

  /// `host` or `host:port`.
  final String host;
  final List<ServerEntry> entries;

  /// By host, in the order the hosts first appear among [servers].
  static List<WorkspaceGroup> group(List<ServerEntry> servers) {
    final order = <String>[];
    final byHost = <String, List<ServerEntry>>{};
    for (final server in servers) {
      final host = hostOf(server.url);
      if (!byHost.containsKey(host)) order.add(host);
      (byHost[host] ??= []).add(server);
    }
    return [for (final host in order) WorkspaceGroup(host, byHost[host]!)];
  }

  static String hostOf(String url) {
    final uri = Uri.tryParse(url);
    if (uri == null || uri.host.isEmpty) return url;
    return uri.hasPort ? '${uri.host}:${uri.port}' : uri.host;
  }
}
