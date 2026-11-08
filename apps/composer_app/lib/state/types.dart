// Plain state types (no behaviour).

import 'package:composer_protocol/composer_protocol.dart';

/// The phone layout's tabs.
enum PhoneTab { chats, projects, pages, server }

/// What the reader screen shows: the page, or the micro app opened over it.
enum ReaderTab { page, app }

/// A page an agent or app changed lately (the Pages tab).
class RecentPage {
  const RecentPage(this.url, this.by);
  final String url;
  final ChangedBy by;
}

/// The Pages tab's search: the query, and its hits once they arrive.
class PageSearch {
  const PageSearch({this.query = '', this.hits, this.error});
  final String query;

  /// Null while loading (or with an empty query).
  final List<SearchHit>? hits;
  final String? error;
}

/// A project search (`GET /api/projects?q=`): the query and what came back.
class ProjectSearch {
  const ProjectSearch({this.query = '', this.hits, this.error});
  final String query;

  /// Null while loading (or with an empty query: then the full list is `ServerStatus.projects`).
  final List<ProjectSummary>? hits;
  final String? error;
}

/// The pipelines on a server (`pipeline.list`) and the templates new ones are made from.
class Pipelines {
  const Pipelines({this.pipelines = const [], this.templates = const [], this.works = const []});
  final List<PipelineSummary> pipelines;

  /// Every work in them, with its stages.
  final List<WorkSummary> works;
  /// What `pipeline.new` makes pipelines from (`site` is the kind).
  final List<PipelineSummary> templates;
}

/// New work being made: waiting for the server, why it failed, or the work's page once made.
class NewWork {
  const NewWork({this.busy = false, this.error, this.created});
  final bool busy;
  final String? error;
  final String? created;
}

/// A server's WebSocket.
enum LinkState { connecting, online, offline }

/// A short message for a snackbar; `id` tells repeats apart.
class Notice {
  const Notice(this.id, this.text);
  final int id;
  final String text;
}

/// Adding a server.
class PairingState {
  const PairingState({this.busy = false, this.error});

  final bool busy;
  final String? error;
}

/// A tapped notification (or one of its buttons).
class NotificationTap {
  const NotificationTap({required this.url, this.session, this.approval, this.page, this.app, this.action});
  final String url;
  final String? session;
  final String? approval;
  final String? page;

  /// A site whose app it opens (a pipeline's board).
  final String? app;

  /// `approve` / `deny`; null for the notification itself.
  final String? action;
}

/// A file picked to attach: a path on this device, or its bytes (where there is no path).
class PickedFile {
  const PickedFile(this.name, {this.path, this.bytes});
  final String name;
  final String? path;
  final List<int>? bytes;
}

enum UploadStatus { uploading, ready, failed }

/// A file attached to the message being written; it uploads as soon as it is picked.
class Upload {
  const Upload({required this.id, required this.file, this.status = UploadStatus.uploading, this.progress = 0, this.attachment, this.error});
  final int id;
  final PickedFile file;
  final UploadStatus status;

  /// 0–1 while uploading.
  final double progress;

  /// What the server stored (ready).
  final Attachment? attachment;
  final String? error;

  Upload copyWith({UploadStatus? status, double? progress, Attachment? attachment, String? error}) => Upload(
        id: id,
        file: file,
        status: status ?? this.status,
        progress: progress ?? this.progress,
        attachment: attachment ?? this.attachment,
        error: error,
      );
}

/// A ticket's branch on the review screen: its changes, and what became of it.
class TicketReviewState {
  const TicketReviewState({
    this.changes,
    this.busy = false,
    this.error,
    this.merged,
    this.discarded = false,
    this.drafts = const [],
    this.comments = const [],
  });

  /// The changes against the project's branch, with what merging does (`ticket.changes`).
  final ProjectDiff? changes;

  /// A command for it is on its way.
  final bool busy;
  final String? error;

  /// The merge commit, once merged.
  final String? merged;
  final bool discarded;

  /// Comments written in the diff view, not sent yet.
  final List<DiffComment> drafts;

  /// The comment lines on the ticket page (`- file:line — message`), as the last send left them.
  final List<String> comments;

  TicketReviewState copyWith({
    ProjectDiff? changes,
    bool? busy,
    String? error,
    String? merged,
    bool? discarded,
    List<DiffComment>? drafts,
    List<String>? comments,
  }) =>
      TicketReviewState(
        changes: changes ?? this.changes,
        busy: busy ?? this.busy,
        error: error,
        merged: merged ?? this.merged,
        discarded: discarded ?? this.discarded,
        drafts: drafts ?? this.drafts,
        comments: comments ?? this.comments,
      );
}
