import 'package:composer_protocol/composer_protocol.dart';

import 'types.dart';

const _keep = Object();

/// What the app knows about a server right now (not persisted).
class ServerStatus {
  const ServerStatus({this.link = LinkState.connecting, this.error, this.retryAt, this.info, this.sessions = const []});

  static const initial = ServerStatus();

  final LinkState link;

  /// Why the last connection attempt failed.
  final String? error;

  /// When the next attempt is due, while offline.
  final DateTime? retryAt;

  final ServerInfo? info;

  /// Newest first.
  final List<SessionInfo> sessions;

  ServerStatus copyWith({
    LinkState? link,
    Object? error = _keep,
    Object? retryAt = _keep,
    Object? info = _keep,
    List<SessionInfo>? sessions,
  }) =>
      ServerStatus(
        link: link ?? this.link,
        error: identical(error, _keep) ? this.error : error as String?,
        retryAt: identical(retryAt, _keep) ? this.retryAt : retryAt as DateTime?,
        info: identical(info, _keep) ? this.info : info as ServerInfo?,
        sessions: sessions ?? this.sessions,
      );
}
