// IO the reducer asks for; run by EffectRunner.

import 'server_entry.dart';

sealed class AppEffect {
  const AppEffect();
}

final class LoadServers extends AppEffect {
  const LoadServers();
}

final class SaveServers extends AppEffect {
  const SaveServers(this.servers);
  final List<ServerEntry> servers;
}

final class Pair extends AppEffect {
  const Pair({required this.url, required this.code, required this.deviceName});
  final String url;
  final String code;
  final String deviceName;
}

/// Open (or replace) the server's connection.
final class Connect extends AppEffect {
  const Connect(this.server);
  final ServerEntry server;
}

final class Disconnect extends AppEffect {
  const Disconnect(this.url);
  final String url;
}

/// Retry now instead of waiting for the backoff.
final class Reconnect extends AppEffect {
  const Reconnect(this.url);
  final String url;
}
