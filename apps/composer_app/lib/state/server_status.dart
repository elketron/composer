import 'package:composer_protocol/composer_protocol.dart';

import 'chat_state.dart';
import 'types.dart';

const _keep = Object();

/// What the app knows about a server right now (not persisted).
class ServerStatus {
  const ServerStatus({this.link = LinkState.connecting, this.error, this.retryAt, this.info, this.sessions = const [], this.approvals = const [], this.chat});

  static const initial = ServerStatus();

  final LinkState link;

  /// Why the last connection attempt failed.
  final String? error;

  /// When the next attempt is due, while offline.
  final DateTime? retryAt;

  final ServerInfo? info;

  /// Newest first.
  final List<SessionInfo> sessions;

  /// Recipes waiting for an answer, from any of the server's sessions.
  final List<ServerMessageApprovalRequest> approvals;

  /// The open session, if any.
  final ChatState? chat;

  SessionInfo? session(String id) => sessions.where((s) => s.id == id).firstOrNull;

  ServerStatus copyWith({
    LinkState? link,
    Object? error = _keep,
    Object? retryAt = _keep,
    Object? info = _keep,
    List<SessionInfo>? sessions,
    List<ServerMessageApprovalRequest>? approvals,
    Object? chat = _keep,
  }) =>
      ServerStatus(
        link: link ?? this.link,
        error: identical(error, _keep) ? this.error : error as String?,
        retryAt: identical(retryAt, _keep) ? this.retryAt : retryAt as DateTime?,
        info: identical(info, _keep) ? this.info : info as ServerInfo?,
        sessions: sessions ?? this.sessions,
        approvals: approvals ?? this.approvals,
        chat: identical(chat, _keep) ? this.chat : chat as ChatState?,
      );
}
