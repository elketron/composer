import 'chat_items.dart';

const _keep = Object();

/// The open session of a server: its transcript so far.
class ChatState {
  const ChatState({required this.session, this.items = const [], this.streaming, this.lastSeq = 0});

  final String session;
  final List<ChatItem> items;
  final Streaming? streaming;

  /// The last event applied; a re-attach asks for what came after it.
  final int lastSeq;

  ChatState copyWith({List<ChatItem>? items, Object? streaming = _keep, int? lastSeq}) => ChatState(
        session: session,
        items: items ?? this.items,
        streaming: identical(streaming, _keep) ? this.streaming : streaming as Streaming?,
        lastSeq: lastSeq ?? this.lastSeq,
      );
}
