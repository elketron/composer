import 'chat_items.dart';
import 'types.dart';

const _keep = Object();

/// The open session of a server: its transcript so far.
class ChatState {
  const ChatState({
    required this.session,
    this.items = const [],
    this.streaming,
    this.lastSeq = 0,
    this.liveAfter = 0,
    this.drafting = false,
    this.uploads = const [],
    this.nextUpload = 1,
    this.loaded = false,
    this.firstSeq = 0,
    this.more = false,
    this.loadingOlder = false,
    this.rowBase = 0,
    this.atBottom = true,
    this.newSince = 0,
  });

  final String session;
  final List<ChatItem> items;
  final Streaming? streaming;

  /// The last event applied; a re-attach asks for what came after it.
  final int lastSeq;

  /// The session's last seq when it was opened: events up to it are replays (not followed).
  final int liveAfter;

  /// A message is being written: the chat doesn't switch away by itself.
  final bool drafting;

  /// Files attached to the message being written.
  final List<Upload> uploads;

  /// The id the next upload gets.
  final int nextUpload;

  /// Its last messages arrived (it opens on its end); then it is attached.
  final bool loaded;

  /// The oldest event shown; older ones come as you scroll up.
  final int firstSeq;

  /// Older messages remain on the server.
  final bool more;
  final bool loadingOlder;

  /// The rows that older messages added in front: rows keep their key as they do.
  final int rowBase;

  /// The view is stuck to the newest line: it follows the stream. Scrolled up, it holds its place.
  final bool atBottom;

  /// Rows that landed while the view was scrolled up; what the "new" pill counts.
  final int newSince;

  ChatState copyWith({
    List<ChatItem>? items,
    Object? streaming = _keep,
    int? lastSeq,
    bool? drafting,
    List<Upload>? uploads,
    int? nextUpload,
    bool? loaded,
    int? firstSeq,
    bool? more,
    bool? loadingOlder,
    int? rowBase,
    bool? atBottom,
    int? newSince,
  }) =>
      ChatState(
        session: session,
        items: items ?? this.items,
        streaming: identical(streaming, _keep) ? this.streaming : streaming as Streaming?,
        lastSeq: lastSeq ?? this.lastSeq,
        liveAfter: liveAfter,
        drafting: drafting ?? this.drafting,
        uploads: uploads ?? this.uploads,
        nextUpload: nextUpload ?? this.nextUpload,
        loaded: loaded ?? this.loaded,
        firstSeq: firstSeq ?? this.firstSeq,
        more: more ?? this.more,
        loadingOlder: loadingOlder ?? this.loadingOlder,
        rowBase: rowBase ?? this.rowBase,
        atBottom: atBottom ?? this.atBottom,
        newSince: newSince ?? this.newSince,
      );
}
