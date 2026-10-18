import 'package:composer_protocol/composer_protocol.dart';

import '../state/actions.dart';
import '../state/effects.dart';
import '../state/server_entry.dart';
import 'server_api.dart';
import 'server_connection.dart';
import 'server_store.dart';

/// Runs the reducer's effects: storage, pairing, and the connection to every server.
class EffectRunner {
  final Map<String, ServerConnection> _connections = {};
  ServerStore? _store;

  /// Opened on first use (the keyring check is async).
  Future<ServerStore> get _servers async => _store ??= await ServerStore.open();

  void run(AppEffect effect, void Function(AppAction) dispatch) {
    switch (effect) {
      case LoadServers():
        _load(dispatch);
      case SaveServers(:final servers):
        _servers.then((store) => store.save(servers));
      case Pair(:final url, :final code, :final deviceName):
        _pair(url, code, deviceName, dispatch);
      case Connect(:final server):
        _connections.remove(server.url)?.close();
        _connections[server.url] = ServerConnection(server, dispatch)..start();
      case Disconnect(:final url):
        _connections.remove(url)?.close();
      case Reconnect(:final url):
        _connections[url]?.retryNow();
      case SendCommand(:final url, :final command):
        _request(url, command, dispatch);
      case CreateSession(:final url, :final agent):
        _request(url, ClientCommandSessionCreate(agent: agent), dispatch).then((result) {
          if (result is CommandResultSession) dispatch(SessionOpened(url, result.session.id));
        });
    }
  }

  /// The connection to a server, for sending commands.
  ServerConnection? connection(String url) => _connections[url];

  /// Null when it failed (reported as `CommandFailed`).
  Future<CommandResult?> _request(String url, ClientCommand command, void Function(AppAction) dispatch) async {
    final connection = _connections[url];
    if (connection == null) {
      dispatch(CommandFailed(url, 'not connected'));
      return null;
    }
    try {
      return await connection.request(command);
    } catch (error) {
      dispatch(CommandFailed(url, describeError(error)));
      return null;
    }
  }

  Future<void> _load(void Function(AppAction) dispatch) async {
    final store = await _servers;
    dispatch(ServersLoaded(await store.load(), store.kind));
  }

  Future<void> _pair(String url, String code, String deviceName, void Function(AppAction) dispatch) async {
    try {
      final token = await redeemCode(url, code, deviceName);
      final info = await fetchInfo(url, token);
      dispatch(PairSucceeded(ServerEntry(url: url, name: info.name, token: token), info));
    } catch (error) {
      dispatch(PairFailed(describeError(error)));
    }
  }
}
