// Turns a session's events into chat items (the TUI's SessionStore::apply, in Dart).


import 'package:composer_protocol/composer_protocol.dart';

import 'chat_items.dart';
import 'chat_rows.dart';
import 'chat_state.dart';
import 'tool_text.dart';

/// The session and seq of a transcript event; null for other messages.
(String, int)? sessionEvent(ServerMessage message) => switch (message) {
      ServerMessageTurnStarted(:final session, :final seq) ||
      ServerMessageTurnQueued(:final session, :final seq) ||
      ServerMessageTurnDone(:final session, :final seq) ||
      ServerMessageThinkingDelta(:final session, :final seq) ||
      ServerMessageMessageDelta(:final session, :final seq) ||
      ServerMessageMessageDone(:final session, :final seq) ||
      ServerMessageToolCall(:final session, :final seq) ||
      ServerMessageToolResult(:final session, :final seq) ||
      ServerMessageAgentNavigated(:final session, :final seq) ||
      ServerMessageSessionCompacted(:final session, :final seq) =>
        (session, seq),
      _ => null,
    };

/// Applies an event of this chat's session; already-seen seqs (replays) are skipped.
ChatState applyChatEvent(ChatState chat, ServerMessage message) {
  final next = _apply(chat, message);
  // What lands while you are scrolled up waits behind the pill; at the bottom it just follows.
  // It counts rows: an event that fills in a row already on the list, or adds none, is nothing new to read.
  final rows = chatRows(next.items).length - chatRows(chat.items).length;
  if (rows > 0 && !chat.atBottom) return next.copyWith(newSince: next.newSince + rows);
  return next;
}

ChatState _apply(ChatState chat, ServerMessage message) {
  final event = sessionEvent(message);
  if (event == null || event.$1 != chat.session || event.$2 <= chat.lastSeq) return chat;
  final next = chat.copyWith(lastSeq: event.$2);
  final streaming = chat.streaming ?? const Streaming();
  return switch (message) {
    ServerMessageThinkingDelta(:final text) =>
      next.copyWith(streaming: Streaming(thinking: streaming.thinking + text, content: streaming.content)),
    ServerMessageMessageDelta(:final text) =>
      next.copyWith(streaming: Streaming(thinking: streaming.thinking, content: streaming.content + text)),
    ServerMessageMessageDone(:final message) => _done(next, message),
    ServerMessageToolCall(:final callId, :final name, :final args, :final parent) => next.copyWith(items: [
        ...chat.items,
        ToolItem(callId: callId, name: name, summary: toolSummary(name, args), detail: toolDetail(name, args), nested: parent != null),
      ]),
    ServerMessageToolResult(:final callId, :final result, :final isError) => next.copyWith(items: _withResult(chat.items, callId, result, isError)),
    ServerMessageSessionCompacted(:final summary, :final messages) =>
      next.copyWith(items: [...chat.items, CompactedItem(summary, messages)]),
    ServerMessageTurnDone(:final cancelled) => next.copyWith(
        streaming: null,
        items: cancelled ? [...chat.items, const NoteItem('cancelled')] : null,
      ),
    _ => next,
  };
}

ChatState _done(ChatState chat, ChatMessage message) => switch (message.role) {
      Role.user => chat.copyWith(items: [...chat.items, UserItem(message.content, source: _source(message.source), attachments: message.attachments)]),
      Role.assistant => chat.copyWith(streaming: null, items: [
          ...chat.items,
          if (message.thinking?.trim().isNotEmpty ?? false) ThinkingItem(message.thinking!),
          if (message.content.trim().isNotEmpty) TextItem(message.content),
        ]),
      Role.system || Role.tool => chat.copyWith(items: [...chat.items, NoteItem(message.content)]),
    };

String? _source(MessageSource? source) => switch (source) {
      MessageSourceApp(:final site) => '$site app',
      MessageSourceWorker(:final session) => 'worker #$session',
      MessageSourceReminder() => 'reminder',
      _ => null,
    };

List<ChatItem> _withResult(List<ChatItem> items, String callId, String result, bool isError) {
  final index = items.lastIndexWhere((i) => i is ToolItem && i.callId == callId);
  if (index < 0) return items;
  final tool = items[index] as ToolItem;
  return [...items]..[index] = ToolItem(
      callId: callId, name: tool.name, summary: tool.summary, detail: tool.detail, nested: tool.nested, result: result, isError: isError);
}

