import 'chat_items.dart';

const _keep = Object();

/// The open session of a server: its transcript so far.
class ChatState {
  const ChatState({required this.session, this.items = const [], this.streaming, this.lastSeq = 0, this.liveAfter = 0, this.drafting = false});

  final String session;
  final List<ChatItem> items;
  final Streaming? streaming;

  /// The last event applied; a re-attach asks for what came after it.
  final int lastSeq;

  /// The session's last seq when it was opened: events up to it are replays (not followed).
  final int liveAfter;

  /// A message is being written: the chat doesn't switch away by itself.
  final bool drafting;

  ChatState copyWith({List<ChatItem>? items, Object? streaming = _keep, int? lastSeq, bool? drafting}) => ChatState(
        session: session,
        items: items ?? this.items,
        streaming: identical(streaming, _keep) ? this.streaming : streaming as Streaming?,
        lastSeq: lastSeq ?? this.lastSeq,
        liveAfter: liveAfter,
        drafting: drafting ?? this.drafting,
      );
}
