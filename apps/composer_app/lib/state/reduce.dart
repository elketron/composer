import 'package:composer_protocol/composer_protocol.dart';

import 'actions.dart';
import 'app_state.dart';
import 'effects.dart';
import 'server_entry.dart';
import 'server_status.dart';
import 'types.dart';

/// The app's reducer: pure, so it is tested without IO.
(AppState, List<AppEffect>) reduce(AppState s, AppAction action) => switch (action) {
      AppStarted() => (s, const [LoadServers()]),
      AppResumed() => (
          s,
          [for (final server in s.servers) if (s.statusOf(server.url).link == LinkState.offline) Reconnect(server.url)],
        ),
      ServersLoaded(:final servers, :final storage) => (
          s.copyWith(
            loaded: true,
            storage: storage,
            servers: servers,
            status: {for (final server in servers) server.url: ServerStatus.initial},
            active: servers.firstOrNull?.url,
          ),
          [for (final server in servers) Connect(server)],
        ),
      ServerSelected(:final url) => s.servers.any((x) => x.url == url) ? (s.copyWith(active: url), const []) : (s, const []),
      PairingReset() => (s.copyWith(pairing: const PairingState()), const []),
      PairRequested(:final url, :final code, :final deviceName) => (
          s.copyWith(pairing: const PairingState(busy: true)),
          [Pair(url: url, code: code, deviceName: deviceName)],
        ),
      PairFailed(:final message) => (s.copyWith(pairing: PairingState(error: message)), const []),
      PairSucceeded(:final server, :final info) => _paired(s, server.withName(info.name), info),
      ServerRemoved(:final url) => _removed(s, url),
      ReconnectRequested(:final url) => (s, [Reconnect(url)]),
      LinkChanged(:final url, :final link, :final error, :final retryAt) => (
          _updateStatus(s, url, (x) => x.copyWith(link: link, error: error, retryAt: retryAt)),
          const [],
        ),
      InfoReceived(:final url, :final info) => _info(s, url, info),
      ServerEvent(:final url, :final message) => (_event(s, url, message), const []),
    };

/// Pairing again with a known server replaces its token.
(AppState, List<AppEffect>) _paired(AppState s, ServerEntry server, ServerInfo info) {
  final known = s.servers.any((x) => x.url == server.url);
  final servers = known ? [for (final x in s.servers) x.url == server.url ? server : x] : [...s.servers, server];
  return (
    s.copyWith(
      servers: servers,
      status: {...s.status, server.url: ServerStatus(info: info)},
      active: server.url,
      pairing: const PairingState(),
    ),
    [SaveServers(servers), Connect(server)],
  );
}

(AppState, List<AppEffect>) _removed(AppState s, String url) {
  final servers = [for (final x in s.servers) if (x.url != url) x];
  final active = s.active == url ? servers.firstOrNull?.url : s.active;
  return (
    s.copyWith(servers: servers, status: {...s.status}..remove(url), active: active),
    [Disconnect(url), SaveServers(servers)],
  );
}

/// Keeps the stored name in step with the server's.
(AppState, List<AppEffect>) _info(AppState s, String url, ServerInfo info) {
  final next = _updateStatus(s, url, (x) => x.copyWith(info: info));
  final entry = s.servers.where((x) => x.url == url).firstOrNull;
  if (entry == null || entry.name == info.name) return (next, const []);
  final servers = [for (final x in s.servers) x.url == url ? x.withName(info.name) : x];
  return (next.copyWith(servers: servers), [SaveServers(servers)]);
}

AppState _event(AppState s, String url, ServerMessage message) => switch (message) {
      ServerMessageOk(result: CommandResultSessions(:final sessions)) =>
        _updateStatus(s, url, (x) => x.copyWith(sessions: _newestFirst(sessions))),
      ServerMessageSessionUpdated(:final session) => _updateStatus(
          s,
          url,
          (x) => x.copyWith(sessions: _newestFirst([session, for (final y in x.sessions) if (y.id != session.id) y])),
        ),
      _ => s,
    };

List<SessionInfo> _newestFirst(List<SessionInfo> sessions) => [...sessions]..sort((a, b) => b.updated.compareTo(a.updated));

AppState _updateStatus(AppState s, String url, ServerStatus Function(ServerStatus) update) {
  final current = s.status[url];
  if (current == null) return s; // removed meanwhile
  return s.copyWith(status: {...s.status, url: update(current)});
}
