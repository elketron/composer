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
  const NewSessionRequested(this.url, {this.agent = 'default', this.project, this.text});
  final String url;
  final String agent;

  /// A name in the projects directory, or a path; null for none.
  final String? project;

  /// A first message, sent once it is open.
  final String? text;
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

/// Set the open session's project (a name in the projects directory), or unset it (null).
final class ProjectChosen extends AppAction {
  const ProjectChosen(this.url, this.project);
  final String url;
  final String? project;
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

/// Ask which external harnesses a project allows and has on (for the chat menu).
final class HarnessesRequested extends AppAction {
  const HarnessesRequested(this.url, this.project);
  final String url;
  final String project;
}

/// Open (or refresh) a ticket's review: its changes against the project's branch.
final class ReviewRequested extends AppAction {
  const ReviewRequested(this.url, this.ticket);
  final String url;
  final String ticket;
}

/// Merge a ticket's branch into the project's checked-out branch.
final class TicketMergeRequested extends AppAction {
  const TicketMergeRequested(this.url, this.ticket);
  final String url;
  final String ticket;
}

/// Throw a ticket's branch and worktree away.
final class TicketDiscardRequested extends AppAction {
  const TicketDiscardRequested(this.url, this.ticket);
  final String url;
  final String ticket;
}

/// The answer to a ticket command: its result, or why it failed.
final class TicketReplied extends AppAction {
  const TicketReplied(this.url, this.ticket, {this.result, this.error});
  final String url;
  final String ticket;
  final CommandResult? result;
  final String? error;
}

/// Turn an external harness (`claude-code`) on or off for a project.
final class HarnessToggled extends AppAction {
  const HarnessToggled(this.url, this.project, this.entry, this.on);
  final String url;
  final String project;
  final String entry;
  final bool on;
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

final class AgentsLoaded extends AppAction {
  const AgentsLoaded(this.url, this.agents);
  final String url;
  final List<AgentSummary> agents;
}

final class ProjectsLoaded extends AppAction {
  const ProjectsLoaded(this.url, this.projects);
  final String url;
  final List<ProjectSummary> projects;
}

/// Show `tab` in the side pane (wide screens); for `changes`, the ticket it shows; for
/// `worker`, the session; for `project`, the project's folder.
final class SideTabSelected extends AppAction {
  const SideTabSelected(this.url, this.tab, {this.ticket, this.session, this.project});
  final String url;
  final SideTab tab;
  final String? ticket;
  final String? session;
  final String? project;
}

/// Fold or unfold the session list (wide screens).
final class SessionsToggled extends AppAction {
  const SessionsToggled();
}

/// The side pane was dragged to `width`.
final class SideResized extends AppAction {
  const SideResized(this.width);
  final double width;
}

/// The saved layout, read at start.
final class LayoutLoaded extends AppAction {
  const LayoutLoaded(this.layout);
  final LayoutPrefs layout;
}

/// Close the side pane, keeping what it showed (wide screens).
final class SideClosed extends AppAction {
  const SideClosed(this.url);
  final String url;
}

/// Search the projects (empty: the full list).
final class ProjectSearchChanged extends AppAction {
  const ProjectSearchChanged(this.url, this.query);
  final String url;
  final String query;
}

final class ProjectsFound extends AppAction {
  const ProjectsFound(this.url, this.query, this.hits, {this.error});
  final String url;
  final String query;
  final List<ProjectSummary> hits;
  final String? error;
}

/// Ask what is going on in a project (its folder, or a name); the reply fills `overviews`.
final class ProjectOverviewRequested extends AppAction {
  const ProjectOverviewRequested(this.url, this.project);
  final String url;
  final String project;
}

/// Narrow the session list and the palette to a project (its folder); null: all.
final class ScopeChanged extends AppAction {
  const ScopeChanged(this.url, this.project);
  final String url;
  final String? project;
}

/// Fetch the sites, agents and projects again (the project counts change).
final class CatalogRequested extends AppAction {
  const CatalogRequested(this.url);
  final String url;
}

/// Ask for the pipelines and their templates; the reply fills `pipelines`.
final class PipelinesRequested extends AppAction {
  const PipelinesRequested(this.url);
  final String url;
}

/// Make a piece of work in `pipeline`; with `template`, first the pipeline itself from that
/// template (`pipeline` is then its name).
final class WorkCreateRequested extends AppAction {
  const WorkCreateRequested(this.url, {required this.pipeline, required this.name, this.project, this.brief, this.template});
  final String url;
  final String pipeline;
  final String name;
  final String? project;
  final String? brief;
  final String? template;
}

/// The answer to `work.new` (and `pipeline.new` before it): the work, or why it failed.
final class WorkReplied extends AppAction {
  const WorkReplied(this.url, {this.work, this.error});
  final String url;
  final WorkSummary? work;
  final String? error;
}

/// The New work dialog closed (or starts over).
final class NewWorkReset extends AppAction {
  const NewWorkReset(this.url);
  final String url;
}
