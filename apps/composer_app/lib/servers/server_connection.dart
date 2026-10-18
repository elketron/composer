import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:math';

import 'package:composer_protocol/composer_protocol.dart';
import 'package:web_socket_channel/io.dart';

import '../state/actions.dart';
import '../state/server_entry.dart';
import '../state/types.dart';
import 'server_api.dart';

/// One server's WebSocket, kept open: reconnects with backoff (1s, 2s, … 30s) and reports
/// everything as actions.
class ServerConnection {
  ServerConnection(this.server, this._dispatch);

  final ServerEntry server;
  final void Function(AppAction) _dispatch;

  IOWebSocketChannel? _channel;
  StreamSubscription<dynamic>? _subscription;
  Timer? _retry;
  int _failures = 0;
  int _nextId = 1;
  bool _closed = false;

  /// Commands waiting for their `ok` / `error`.
  final Map<int, Completer<CommandResult>> _pending = {};

  void start() => _connect();

  /// Retry now (a button, or the app coming back to the foreground).
  void retryNow() {
    if (_closed || _channel != null) return;
    _retry?.cancel();
    _failures = 0;
    _connect();
  }

  /// Sends a command without waiting for the reply (it still arrives as a `ServerEvent`).
  int send(ClientCommand command) {
    final id = _nextId++;
    _channel?.sink.add(jsonEncode(ClientRequest(id: id, command: command).toJson()));
    return id;
  }

  /// Sends a command and completes with its result; fails with a [ServerError] on an `error`
  /// reply, or when offline or the connection drops first.
  Future<CommandResult> request(ClientCommand command) {
    if (_channel == null) return Future.error(const ServerError('offline'));
    final completer = Completer<CommandResult>();
    _pending[send(command)] = completer;
    return completer.future;
  }

  void close() {
    _closed = true;
    _failPending('disconnected');
    _retry?.cancel();
    _subscription?.cancel();
    _channel?.sink.close();
    _channel = null;
  }

  Future<void> _connect() async {
    if (_closed) return;
    _dispatch(LinkChanged(server.url, LinkState.connecting));
    try {
      final info = await fetchInfo(server.url, server.token);
      if (_closed) return;
      _dispatch(InfoReceived(server.url, info));
      final channel = IOWebSocketChannel.connect(
        wsUri(server.url),
        headers: {HttpHeaders.authorizationHeader: 'Bearer ${server.token}'},
        pingInterval: const Duration(seconds: 20),
        connectTimeout: const Duration(seconds: 10),
      );
      await channel.ready;
      if (_closed) {
        channel.sink.close();
        return;
      }
      _channel = channel;
      _failures = 0;
      _subscription = channel.stream.listen(_receive, onDone: _lost, onError: (_) => _lost());
      _dispatch(LinkChanged(server.url, LinkState.online));
      send(const ClientCommandSessionList());
    } catch (error) {
      _scheduleRetry(describeError(error));
    }
  }

  void _receive(dynamic data) {
    if (data is! String) return;
    final json = jsonDecode(data);
    if (json is! Map<String, Object?>) return;
    final message = ServerMessage.fromJson(json);
    switch (message) {
      case ServerMessageOk(:final id, :final result):
        _pending.remove(id)?.complete(result);
      case ServerMessageError(:final id?, :final message):
        _pending.remove(id)?.completeError(ServerError(message));
      default:
    }
    _dispatch(ServerEvent(server.url, message));
  }

  void _failPending(String reason) {
    for (final completer in _pending.values) {
      completer.completeError(ServerError(reason));
    }
    _pending.clear();
  }

  void _lost() {
    _failPending('connection lost');
    _subscription?.cancel();
    _subscription = null;
    _channel = null;
    if (!_closed) _scheduleRetry('connection lost');
  }

  void _scheduleRetry(String error) {
    if (_closed) return;
    final delay = Duration(seconds: min(30, 1 << min(_failures, 5)));
    _failures++;
    _dispatch(LinkChanged(server.url, LinkState.offline, error: error, retryAt: DateTime.now().add(delay)));
    _retry = Timer(delay, _connect);
  }
}
