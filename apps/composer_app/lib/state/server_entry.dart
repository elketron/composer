/// A paired server, as persisted.
class ServerEntry {
  const ServerEntry({required this.url, required this.name, required this.token});

  /// Base URL of one workspace on a server (`https://host.tailnet.ts.net/w/default`); also its id
  /// in the app.
  final String url;

  /// From `/api/info` (the server's name or hostname).
  final String name;

  /// This device's token, from pairing.
  final String token;

  factory ServerEntry.fromJson(Map<String, Object?> json) => ServerEntry(
        url: inWorkspace(json['url'] as String),
        name: json['name'] as String,
        token: json['token'] as String,
      );

  /// A server saved before workspaces (`http://host:7420`, no path) is that server's `default`
  /// workspace now: `composer migrate` moves the old home, and the devices paired to it, there.
  /// Any URL with a path is left as it is.
  static String inWorkspace(String url) {
    final uri = Uri.tryParse(url);
    if (uri == null || !uri.hasAuthority) return url;
    if (uri.path.isNotEmpty && uri.path != '/') return url;
    return '${url.replaceAll(RegExp(r'/+$'), '')}/w/default';
  }

  Map<String, Object?> toJson() => {'url': url, 'name': name, 'token': token};

  ServerEntry withName(String name) => ServerEntry(url: url, name: name, token: token);
}
