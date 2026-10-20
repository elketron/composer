// Plain state types (no behaviour).

import 'package:composer_protocol/composer_protocol.dart';

/// The phone layout's tabs.
enum PhoneTab { chats, pages, server }

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

/// A server's WebSocket.
enum LinkState { connecting, online, offline }

/// Where device tokens are kept.
enum StorageKind {
  /// The platform keyring: Secret Service, Android Keystore, Windows Credential Manager.
  keyring,

  /// A file only you can read (0600), like `composer login` uses: no keyring was available.
  file,
}

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
  const NotificationTap({required this.url, this.session, this.approval, this.page, this.action});
  final String url;
  final String? session;
  final String? approval;
  final String? page;

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
