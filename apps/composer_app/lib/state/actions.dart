// Everything that can happen to the app state. Dispatched by widgets and by effects.

import 'package:composer_protocol/composer_protocol.dart';

import 'server_entry.dart';
import 'types.dart';

sealed class AppAction {
  const AppAction();
}

final class AppStarted extends AppAction {
  const AppStarted();
}

/// Back in the foreground: retry offline servers now.
final class AppResumed extends AppAction {
  const AppResumed();
}

final class ServersLoaded extends AppAction {
  const ServersLoaded(this.servers, this.storage);
  final List<ServerEntry> servers;
  final StorageKind storage;
}

final class ServerSelected extends AppAction {
  const ServerSelected(this.url);
  final String url;
}

/// The add-server form opened: forget an earlier error.
final class PairingReset extends AppAction {
  const PairingReset();
}

final class PairRequested extends AppAction {
  const PairRequested({required this.url, required this.code, required this.deviceName});
  final String url;
  final String code;
  final String deviceName;
}

final class PairSucceeded extends AppAction {
  const PairSucceeded(this.server, this.info);
  final ServerEntry server;
  final ServerInfo info;
}

final class PairFailed extends AppAction {
  const PairFailed(this.message);
  final String message;
}

final class ServerRemoved extends AppAction {
  const ServerRemoved(this.url);
  final String url;
}

final class ReconnectRequested extends AppAction {
  const ReconnectRequested(this.url);
  final String url;
}

final class LinkChanged extends AppAction {
  const LinkChanged(this.url, this.link, {this.error, this.retryAt});
  final String url;
  final LinkState link;
  final String? error;
  final DateTime? retryAt;
}

final class InfoReceived extends AppAction {
  const InfoReceived(this.url, this.info);
  final String url;
  final ServerInfo info;
}

/// A message from a server's WebSocket.
final class ServerEvent extends AppAction {
  const ServerEvent(this.url, this.message);
  final String url;
  final ServerMessage message;
}
