import 'chat_items.dart';
import 'types.dart';

const _keep = Object();

/// The open session of a server: its transcript so far.
class ChatState {
  const ChatState({required this.session, this.items = const [], this.streaming, this.lastSeq = 0, this.liveAfter = 0, this.drafting = false, this.uploads = const [], this.nextUpload = 1});

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

  ChatState copyWith({List<ChatItem>? items, Object? streaming = _keep, int? lastSeq, bool? drafting, List<Upload>? uploads, int? nextUpload}) => ChatState(
        session: session,
        items: items ?? this.items,
        streaming: identical(streaming, _keep) ? this.streaming : streaming as Streaming?,
        lastSeq: lastSeq ?? this.lastSeq,
        liveAfter: liveAfter,
        drafting: drafting ?? this.drafting,
        uploads: uploads ?? this.uploads,
        nextUpload: nextUpload ?? this.nextUpload,
      );
}
