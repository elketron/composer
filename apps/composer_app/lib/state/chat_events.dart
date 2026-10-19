// Turns a session's events into chat items (the TUI's SessionStore::apply, in Dart).

import 'dart:convert';

import 'package:composer_protocol/composer_protocol.dart';

import 'chat_items.dart';
import 'chat_state.dart';

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
    ServerMessageToolCall(:final callId, :final name, :final args) =>
      next.copyWith(items: [...chat.items, ToolItem(callId: callId, name: name, summary: summarizeArgs(args))]),
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
      Role.user => chat.copyWith(items: [...chat.items, UserItem(message.content, source: _source(message.source))]),
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
  return [...items]..[index] = ToolItem(callId: callId, name: tool.name, summary: tool.summary, result: result, isError: isError);
}

/// The interesting argument of a tool call, on one line (as the TUI shows it).
String summarizeArgs(Object? args) {
  if (args is! Map) return args == null ? '' : jsonEncode(args);
  String? get(String key) => args[key] is String ? args[key] as String : null;
  final site = get('site'), recipe = get('recipe');
  if (site != null && recipe != null) {
    final extra = (args['args'] is List ? args['args'] as List : const []).whereType<String>().join(' ');
    return '$site › $recipe $extra'.trimRight();
  }
  for (final key in ['url', 'query', 'path', 'ticket', 'status', 'site']) {
    final value = get(key);
    if (value != null) return value;
  }
  return args.isEmpty ? '' : jsonEncode(args);
}
