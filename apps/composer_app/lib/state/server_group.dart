import 'server_entry.dart';

/// The paired servers of one host (`host` or `host:port`), grouped by the machine that serves them.
class ServerGroup {
  const ServerGroup(this.host, this.entries);

  /// `host` or `host:port`.
  final String host;
  final List<ServerEntry> entries;

  /// By host, in the order the hosts first appear among [servers].
  static List<ServerGroup> group(List<ServerEntry> servers) {
    final order = <String>[];
    final byHost = <String, List<ServerEntry>>{};
    for (final server in servers) {
      final host = hostOf(server.url);
      if (!byHost.containsKey(host)) order.add(host);
      (byHost[host] ??= []).add(server);
    }
    return [for (final host in order) ServerGroup(host, byHost[host]!)];
  }

  static String hostOf(String url) {
    final uri = Uri.tryParse(url);
    if (uri == null || uri.host.isEmpty) return url;
    return uri.hasPort ? '${uri.host}:${uri.port}' : uri.host;
  }
}
