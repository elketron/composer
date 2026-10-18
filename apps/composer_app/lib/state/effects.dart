// IO the reducer asks for; run by EffectRunner.

import 'package:composer_protocol/composer_protocol.dart';

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

/// Send a command; a failure comes back as `CommandFailed`.
final class SendCommand extends AppEffect {
  const SendCommand(this.url, this.command);
  final String url;
  final ClientCommand command;
}

/// `session.create`, then open it.
final class CreateSession extends AppEffect {
  const CreateSession(this.url, this.agent);
  final String url;
  final String agent;
}

final class FetchPage extends AppEffect {
  const FetchPage(this.url, this.page);
  final String url;
  final String page;
}

final class FetchSites extends AppEffect {
  const FetchSites(this.url);
  final String url;
}
