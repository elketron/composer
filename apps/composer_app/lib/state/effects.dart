// IO the reducer asks for; run by EffectRunner.

import 'package:composer_protocol/composer_protocol.dart';

import 'server_entry.dart';
import 'types.dart';

sealed class AppEffect {
  const AppEffect();
}

final class LoadServers extends AppEffect {
  const LoadServers();
}

final class SaveServers extends AppEffect {
  const SaveServers(this.servers);
  final List<ServerEntry> servers;
}

final class Pair extends AppEffect {
  const Pair({required this.url, required this.code, required this.deviceName});
  final String url;
  final String code;
  final String deviceName;
}

/// Open (or replace) the server's connection.
final class Connect extends AppEffect {
  const Connect(this.server);
  final ServerEntry server;
}

final class Disconnect extends AppEffect {
  const Disconnect(this.url);
  final String url;
}

/// Retry now instead of waiting for the backoff.
final class Reconnect extends AppEffect {
  const Reconnect(this.url);
  final String url;
}

/// Send a command; a failure comes back as `CommandFailed`.
final class SendCommand extends AppEffect {
  const SendCommand(this.url, this.command);
  final String url;
  final ClientCommand command;
}

/// A `ticket.*` command; the reply comes back as `TicketReplied`.
final class TicketCommand extends AppEffect {
  const TicketCommand(this.url, this.ticket, this.command);
  final String url;
  final String ticket;
  final ClientCommand command;
}

/// `session.create`, then open it.
final class CreateSession extends AppEffect {
  const CreateSession(this.url, this.agent, {this.project, this.text});
  final String url;
  final String agent;
  final String? project;

  /// Sent as its first message once it is open.
  final String? text;
}

/// A system notification; `key` identifies it (to replace or withdraw it).
final class Notify extends AppEffect {
  const Notify({
    required this.key,
    required this.title,
    required this.body,
    required this.url,
    this.session,
    this.approval,
    this.page,
  });
  final String key;
  final String title;
  final String body;

  /// Where tapping it leads: the server, and the session if any.
  final String url;
  final String? session;

  /// An approval request id: the notification gets Approve / Deny.
  final String? approval;

  /// A page to open when there is no session (a reminder).
  final String? page;
}

final class CancelNotification extends AppEffect {
  const CancelNotification(this.key);
  final String key;
}

/// Android: keep the app running in the background while there are servers to stay connected to.
final class KeepAlive extends AppEffect {
  const KeepAlive(this.servers);
  final int servers;
}

final class FetchPage extends AppEffect {
  const FetchPage(this.url, this.page);
  final String url;
  final String page;
}

/// The sites, agents and projects (`SitesLoaded`, `AgentsLoaded`, `ProjectsLoaded`).
final class FetchCatalog extends AppEffect {
  const FetchCatalog(this.url);
  final String url;
}

/// `GET /api/search` (`SearchLoaded`).
final class SearchPages extends AppEffect {
  const SearchPages(this.url, this.query);
  final String url;
  final String query;
}

/// Upload a picked file to a session's attachments (reports `UploadProgressed` and `UploadFinished`).
final class UploadAttachment extends AppEffect {
  const UploadAttachment(this.url, this.session, this.id, this.file);
  final String url;
  final String session;
  final int id;
  final PickedFile file;
}

/// Keep the wide layout (collapsed session list, side pane width) for the next run.
final class SaveLayout extends AppEffect {
  const SaveLayout(this.layout);
  final LayoutPrefs layout;
}

/// `GET /api/projects?q=` (`ProjectsFound`).
final class SearchProjects extends AppEffect {
  const SearchProjects(this.url, this.query);
  final String url;
  final String query;
}

/// `pipeline.new` (with `template`) then `work.new`; the outcome comes back as `WorkReplied`.
final class CreateWork extends AppEffect {
  const CreateWork(this.url, this.work, {this.template});
  final String url;
  final ClientCommandWorkNew work;
  final String? template;
}

/// `work.run`; says what started (or why not), then refreshes `project`'s overview.
final class RunWork extends AppEffect {
  const RunWork(this.url, this.command, {this.project});
  final String url;
  final ClientCommandWorkRun command;
  final String? project;
}
