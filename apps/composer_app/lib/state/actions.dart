// Everything that can happen to the app state. Dispatched by widgets and by effects.

import 'package:composer_protocol/composer_protocol.dart';

import 'frame_size.dart';
import 'live_view_size.dart';
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
  const ServersLoaded(this.servers);
  final List<ServerEntry> servers;
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

/// Scrolled up to the oldest message shown: fetch the ones before it.
final class OlderRequested extends AppAction {
  const OlderRequested(this.url, this.session);
  final String url;
  final String session;
}

/// `session.history`'s reply: the events before `before` (null: the chat's last messages, after
/// which it attaches). `events` null: it failed (an older server), so the whole chat replays.
final class HistoryLoaded extends AppAction {
  const HistoryLoaded(this.url, this.session, this.before, this.events, {this.more = false});
  final String url;
  final String session;
  final int? before;
  final List<ServerMessage>? events;
  final bool more;
}

/// The chat list stopped at the newest line (true) or the user scrolled up off it (false).
final class ViewScrolled extends AppAction {
  const ViewScrolled(this.url, this.session, {required this.atBottom});
  final String url;
  final String session;
  final bool atBottom;
}

/// Leave the chat (detach).
final class SessionClosed extends AppAction {
  const SessionClosed(this.url);
  final String url;
}

final class NewSessionRequested extends AppAction {
  const NewSessionRequested(this.url, {this.kind = RunKind.agentic, this.site});
  final String url;
  final RunKind kind;

  /// The site it starts in; null for none.
  final String? site;
}

final class MessageSubmitted extends AppAction {
  const MessageSubmitted(this.url, this.text);
  final String url;
  final String text;
}

/// The composer went from empty to holding a draft, or back.
final class DraftChanged extends AppAction {
  const DraftChanged(this.url, {required this.drafting});
  final String url;
  final bool drafting;
}

/// Files picked to attach to the open chat's message; each starts uploading.
final class FilesPicked extends AppAction {
  const FilesPicked(this.url, this.files);
  final String url;
  final List<PickedFile> files;
}

final class UploadProgressed extends AppAction {
  const UploadProgressed(this.url, this.id, this.progress);
  final String url;
  final int id;
  final double progress;
}

/// An upload ended: stored (`attachment`) or not (`error`).
final class UploadFinished extends AppAction {
  const UploadFinished(this.url, this.id, {this.attachment, this.error});
  final String url;
  final int id;
  final Attachment? attachment;
  final String? error;
}

final class UploadRetried extends AppAction {
  const UploadRetried(this.url, this.id);
  final String url;
  final int id;
}

final class UploadRemoved extends AppAction {
  const UploadRemoved(this.url, this.id);
  final String url;
  final int id;
}

/// Ask which models the provider has (for the model sheet).
final class ModelsRequested extends AppAction {
  const ModelsRequested(this.url);
  final String url;
}

/// The open session's model; null: the server's again.
final class ModelChosen extends AppAction {
  const ModelChosen(this.url, this.model);
  final String url;
  final String? model;
}

/// Compact the open session: save memory, then summarize older messages.
final class CompactRequested extends AppAction {
  const CompactRequested(this.url);
  final String url;
}

/// The phone layout's tab.
final class TabSelected extends AppAction {
  const TabSelected(this.tab);
  final PhoneTab tab;
}

/// The Pages tab's search field changed.
final class SearchChanged extends AppAction {
  const SearchChanged(this.url, this.query);
  final String url;
  final String query;
}

final class SearchLoaded extends AppAction {
  const SearchLoaded(this.url, this.query, this.hits, {this.error});
  final String url;
  final String query;
  final List<SearchHit> hits;
  final String? error;
}

final class CancelRequested extends AppAction {
  const CancelRequested(this.url);
  final String url;
}

final class ApprovalAnswered extends AppAction {
  const ApprovalAnswered(this.url, this.request, this.decision);
  final String url;
  final String request;
  final ApprovalDecision decision;
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

/// Fetch the sites again.
final class CatalogRequested extends AppAction {
  const CatalogRequested(this.url);
  final String url;
}

/// Enter a site (`site.set`), or leave it (`site` null). For `session`, else the open chat's.
final class SiteChosen extends AppAction {
  const SiteChosen(this.url, this.site, {this.session});
  final String url;
  final String? site;
  final String? session;
}

/// The device's saved preferences were read (null: nothing saved).
final class PreferencesLoaded extends AppAction {
  const PreferencesLoaded({this.liveViewSize});
  final LiveViewSize? liveViewSize;
}

/// The live view was dragged to a size (the nearest of the three).
final class LiveViewResized extends AppAction {
  const LiveViewResized(this.size);
  final LiveViewSize size;
}

/// The live view's page reported the size of the application's screen (the first frame, or a
/// change of resolution): the view's heights follow its shape.
final class LiveViewFrameSized extends AppAction {
  const LiveViewFrameSized(this.frame);
  final FrameSize frame;
}

/// The handle was tapped: the live view steps to the next size.
final class LiveViewStepped extends AppAction {
  const LiveViewStepped();
}
