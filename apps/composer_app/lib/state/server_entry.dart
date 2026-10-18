/// A paired server, as persisted.
class ServerEntry {
  const ServerEntry({required this.url, required this.name, required this.token});

  /// Base URL (`https://host.tailnet.ts.net`); also the server's id in the app.
  final String url;

  /// From `/api/info` (the server's name or hostname).
  final String name;

  /// This device's token, from pairing.
  final String token;

  factory ServerEntry.fromJson(Map<String, Object?> json) =>
      ServerEntry(url: json['url'] as String, name: json['name'] as String, token: json['token'] as String);

  Map<String, Object?> toJson() => {'url': url, 'name': name, 'token': token};

  ServerEntry withName(String name) => ServerEntry(url: url, name: name, token: token);
}
