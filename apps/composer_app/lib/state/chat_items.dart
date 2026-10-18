// What a chat shows, built from a session's events (plain data).

sealed class ChatItem {
  const ChatItem();
}

/// A user-side message: yours, a micro app's, or a finished worker's report.
final class UserItem extends ChatItem {
  const UserItem(this.text, {this.source});
  final String text;

  /// `design app`, `worker #b2c3`; null for your own.
  final String? source;
}

final class ThinkingItem extends ChatItem {
  const ThinkingItem(this.text);
  final String text;
}

/// The agent's reply (markdown).
final class TextItem extends ChatItem {
  const TextItem(this.text);
  final String text;
}

final class ToolItem extends ChatItem {
  const ToolItem({required this.callId, required this.name, required this.summary, this.result, this.isError = false});
  final String callId;
  final String name;

  /// The interesting argument, on one line.
  final String summary;

  /// Null while it runs.
  final String? result;
  final bool isError;
}

/// System notes: cancelled turns, system and tool messages.
final class NoteItem extends ChatItem {
  const NoteItem(this.text);
  final String text;
}

final class CompactedItem extends ChatItem {
  const CompactedItem(this.summary, this.messages);
  final String summary;
  final int messages;
}

/// The reply being streamed.
class Streaming {
  const Streaming({this.thinking = '', this.content = ''});
  final String thinking;
  final String content;
}
