import 'package:composer_protocol/composer_protocol.dart';

import '../state/actions.dart';
import '../state/effects.dart';
import '../state/server_entry.dart';
import 'background_keeper.dart';
import 'notifier.dart';
import 'server_api.dart';
import 'server_connection.dart';
import 'server_store.dart';

/// Runs the reducer's effects: storage, pairing, and the connection to every server.
class EffectRunner {
  EffectRunner(this._notifier);

  final Notifier _notifier;
  final _keeper = BackgroundKeeper();
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
      case Notify():
        _notifier.show(effect);
      case CancelNotification(:final key):
        _notifier.cancel(key);
      case KeepAlive(:final servers):
        _keeper.keep(servers).catchError((_) {});
      case FetchPage(:final url, :final page):
        _fetchPage(url, page, dispatch);
      case FetchCatalog(:final url):
        final server = _connections[url]?.server;
        if (server != null) {
          fetchSites(url, server.token).then((sites) => dispatch(SitesLoaded(url, sites)), onError: (_) {});
          fetchAgents(url, server.token).then((agents) => dispatch(AgentsLoaded(url, agents)), onError: (_) {});
          fetchProjects(url, server.token).then((projects) => dispatch(ProjectsLoaded(url, projects)), onError: (_) {});
        }
      case UploadAttachment(:final url, :final session, :final id, :final file):
        final server = _connections[url]?.server;
        if (server == null) {
          dispatch(UploadFinished(url, id, error: 'not connected'));
          return;
        }
        var shown = 0.0;
        uploadAttachment(url, server.token, session, file, (progress) {
          // A few steps are enough for a progress bar.
          if (progress - shown >= 0.05 || progress >= 1) {
            shown = progress;
            dispatch(UploadProgressed(url, id, progress));
          }
        }).then(
          (attachment) => dispatch(UploadFinished(url, id, attachment: attachment)),
          onError: (Object e) => dispatch(UploadFinished(url, id, error: e is ServerError ? e.message : '$e')),
        );
      case SearchPages(:final url, :final query):
        final server = _connections[url]?.server;
        if (server != null) {
          searchPages(url, server.token, query).then(
            (hits) => dispatch(SearchLoaded(url, query, hits)),
            onError: (Object e) => dispatch(SearchLoaded(url, query, const [], error: describeError(e))),
          );
        }
      case CreateSession(:final url, :final agent, :final project):
        _request(url, ClientCommandSessionCreate(agent: agent, project: project), dispatch).then((result) {
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

  Future<void> _fetchPage(String url, String page, void Function(AppAction) dispatch) async {
    final server = _connections[url]?.server;
    if (server == null) return dispatch(PageFailed(url, page, 'not connected'));
    try {
      dispatch(PageLoaded(url, page, await fetchPage(url, server.token, page)));
    } catch (error) {
      dispatch(PageFailed(url, page, describeError(error)));
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
