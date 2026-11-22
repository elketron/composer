import 'package:composer_protocol/composer_protocol.dart';

// What a chat shows, built from a session's events (plain data).

sealed class ChatItem {
  const ChatItem();
}

/// A user-side message: yours, or one that came from elsewhere (`source`).
final class UserItem extends ChatItem {
  const UserItem(this.text, {this.source, this.attachments = const []});
  final String text;

  /// Files sent with it.
  final List<Attachment> attachments;

  /// `watch`; null for your own.
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
  const ToolItem({required this.callId, required this.name, required this.summary, this.detail, this.nested = false, this.result, this.isError = false});
  final String callId;
  final String name;

  /// The interesting argument, on one line.
  final String summary;

  /// What it changes (an edit's diff, a to-do list), shown with the result.
  final String? detail;

  /// Made inside another call (a Claude Code sub-agent's).
  final bool nested;

  /// Null while it runs.
  final String? result;
  final bool isError;
}

/// System notes: cancelled turns, system and tool messages.
final class NoteItem extends ChatItem {
  const NoteItem(this.text);
  final String text;
}

/// A decision of system-1 (the `[system1]` model) on an approval: `system-1 approved …`.
final class System1Item extends ChatItem {
  const System1Item(this.text);
  final String text;
}

/// A background process the agent started ended; `text` is what the agent was told.
final class ProcessEndedItem extends ChatItem {
  const ProcessEndedItem(this.command, this.text);
  final String command;
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

/// What the chat shows, one entry per row: an item as is, or a turn's steps folded into one row.
sealed class ChatRow {
  const ChatRow();
}

final class ItemRow extends ChatRow {
  const ItemRow(this.item);
  final ChatItem item;
}

/// Consecutive tool calls and thoughts.
final class StepsRow extends ChatRow {
  const StepsRow(this.steps, this.summary);

  /// `ToolItem`s and `ThinkingItem`s, in order.
  final List<ChatItem> steps;

  /// `read 1 page, wrote 0001.md`.
  final String summary;
}
