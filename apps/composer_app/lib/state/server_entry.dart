/// A paired server, as persisted.
class ServerEntry {
  const ServerEntry({
    required this.url,
    required this.name,
    required this.token,
    this.workspace = '',
    this.color = '',
  });

  /// Base URL of one workspace on a server (`https://host.tailnet.ts.net/w/default`); also its id
  /// in the app.
  final String url;

  /// From `/api/info` (the server's name or hostname).
  final String name;

  /// This device's token, from pairing.
  final String token;

  /// From `/api/info`: which workspace this URL is (`''` until it answers, or on an old server).
  final String workspace;

  /// From `/api/info`: the workspace's colour, `#rrggbb` (`''` until it answers, or an old server).
  final String color;

  factory ServerEntry.fromJson(Map<String, Object?> json) => ServerEntry(
        url: inWorkspace(json['url'] as String),
        name: json['name'] as String,
        token: json['token'] as String,
        workspace: json['workspace'] as String? ?? '',
        color: json['color'] as String? ?? '',
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

  /// The workspace in a workspace URL (`/w/<name>`); `default` for one without it.
  static String workspaceIn(String url) {
    final segments = Uri.tryParse(url)?.pathSegments ?? const [];
    final at = segments.indexOf('w');
    if (at < 0 || at + 1 >= segments.length) return 'default';
    return segments[at + 1];
  }

  Map<String, Object?> toJson() => {
        'url': url,
        'name': name,
        'token': token,
        'workspace': workspace,
        'color': color,
      };

  /// A copy with what the server just said about itself.
  ServerEntry withInfo({
    required String name,
    required String workspace,
    required String color,
  }) =>
      ServerEntry(url: url, name: name, token: token, workspace: workspace, color: color);

  ServerEntry withName(String name) => ServerEntry(
        url: url,
        name: name,
        token: token,
        workspace: workspace,
        color: color,
      );
}
