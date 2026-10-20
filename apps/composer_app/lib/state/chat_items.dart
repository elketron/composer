import 'package:composer_protocol/composer_protocol.dart';

// What a chat shows, built from a session's events (plain data).

sealed class ChatItem {
  const ChatItem();
}

/// A user-side message: yours, a micro app's, or a finished worker's report.
final class UserItem extends ChatItem {
  const UserItem(this.text, {this.source, this.attachments = const []});
  final String text;

  /// Files sent with it.
  final List<Attachment> attachments;

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

/// What the chat shows, one entry per row: an item as is, a turn's steps folded into one row,
/// or a worker's report as a card.
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

  /// `read 1 page, wrote 0001.md, started a worker`.
  final String summary;
}

/// `[worker #96a1 finished ticket 0001: done]` and what follows, as the server writes it.
final class WorkerRow extends ChatRow {
  const WorkerRow({required this.worker, required this.ticket, required this.status, required this.summary, this.result, this.branch});
  final String worker;
  final String ticket;

  /// `done` or `failed`.
  final String status;
  final String summary;

  /// The ticket page holding the Result.
  final String? result;

  /// `ticket/<id>`, when its changes are on their own branch (a git project).
  final String? branch;
}
