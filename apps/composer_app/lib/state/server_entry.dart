/// A paired server, as persisted.
class ServerEntry {
  const ServerEntry({
    required this.url,
    required this.name,
    required this.token,
  });

  /// Base URL of the server (`https://host.tailnet.ts.net`); also its id in the app.
  final String url;

  /// From `/api/info` (the server's name or hostname).
  final String name;

  /// This device's token, from pairing.
  final String token;

  factory ServerEntry.fromJson(Map<String, Object?> json) => ServerEntry(
        url: withoutWorkspace(json['url'] as String),
        name: json['name'] as String,
        token: json['token'] as String,
      );

  /// A server has one home now: a URL saved with a workspace path (`…/w/<name>`) is the server's
  /// own URL without it. Any other URL is left as it is.
  static String withoutWorkspace(String url) {
    final uri = Uri.tryParse(url);
    if (uri == null || !uri.hasAuthority) return url;
    final segments = uri.pathSegments.where((s) => s.isNotEmpty).toList();
    if (segments.length != 2 || segments.first != 'w') return url;
    return uri.replace(path: '').toString();
  }

  Map<String, Object?> toJson() => {
        'url': url,
        'name': name,
        'token': token,
      };

  ServerEntry withName(String name) => ServerEntry(url: url, name: name, token: token);
}
