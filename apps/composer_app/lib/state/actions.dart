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

/// The app came to the front (retry offline servers now) or went to the background (notify
/// what is pending instead of showing it).
final class ForegroundChanged extends AppAction {
  const ForegroundChanged(this.foreground);
  final bool foreground;
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

/// Open a session's chat (attach).
final class SessionOpened extends AppAction {
  const SessionOpened(this.url, this.session);
  final String url;
  final String session;
}

/// Leave the chat (detach).
final class SessionClosed extends AppAction {
  const SessionClosed(this.url);
  final String url;
}

final class NewSessionRequested extends AppAction {
  const NewSessionRequested(this.url, {this.agent = 'default'});
  final String url;
  final String agent;
}

final class MessageSubmitted extends AppAction {
  const MessageSubmitted(this.url, this.text);
  final String url;
  final String text;
}

final class CancelRequested extends AppAction {
  const CancelRequested(this.url);
  final String url;
}

final class ApprovalAnswered extends AppAction {
  const ApprovalAnswered(this.url, this.request, {required this.approve});
  final String url;
  final String request;
  final bool approve;
}

/// A command the server refused, or that couldn't be sent.
final class CommandFailed extends AppAction {
  const CommandFailed(this.url, this.message);
  final String url;
  final String message;
}

/// Tell the user something (a snackbar).
final class NoticeShown extends AppAction {
  const NoticeShown(this.text);
  final String text;
}

/// Open a page in the reader.
final class PageRequested extends AppAction {
  const PageRequested(this.url, this.page);
  final String url;
  final String page;
}

final class PageBack extends AppAction {
  const PageBack(this.url);
  final String url;
}

final class PageForward extends AppAction {
  const PageForward(this.url);
  final String url;
}

final class PageReloadRequested extends AppAction {
  const PageReloadRequested(this.url);
  final String url;
}

final class PageLoaded extends AppAction {
  const PageLoaded(this.url, this.requested, this.page);
  final String url;
  final String requested;
  final PageResponse page;
}

final class PageFailed extends AppAction {
  const PageFailed(this.url, this.requested, this.error);
  final String url;
  final String requested;
  final String error;
}

final class FollowToggled extends AppAction {
  const FollowToggled(this.url);
  final String url;
}

/// Show a site's micro app in the reader.
final class AppRequested extends AppAction {
  const AppRequested(this.url, this.site);
  final String url;
  final String site;
}

/// Close the reader (desktop pane).
final class ReaderClosed extends AppAction {
  const ReaderClosed(this.url);
  final String url;
}

final class SitesLoaded extends AppAction {
  const SitesLoaded(this.url, this.sites);
  final String url;
  final List<SiteSummary> sites;
}
