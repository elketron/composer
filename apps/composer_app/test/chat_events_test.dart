import 'package:composer_app/state/chat_events.dart';
import 'package:composer_app/state/tool_text.dart';
import 'package:composer_app/state/chat_items.dart';
import 'package:composer_app/state/chat_state.dart';
import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter_test/flutter_test.dart';

ChatState play(List<ServerMessage> events, {ChatState chat = const ChatState(session: 'a')}) =>
    events.fold(chat, applyChatEvent);

void main() {
  test('a turn: your message, streamed thinking and text, a tool call, the reply', () {
    var chat = play([
      ServerMessageMessageDone(session: 'a', seq: 1, message: const ChatMessage(role: Role.user, content: 'hi')),
      ServerMessageTurnStarted(session: 'a', seq: 2),
      ServerMessageThinkingDelta(session: 'a', seq: 3, text: 'hm'),
      ServerMessageMessageDelta(session: 'a', seq: 4, text: 'Hel'),
      ServerMessageMessageDelta(session: 'a', seq: 5, text: 'lo'),
    ]);
    expect(chat.streaming!.thinking, 'hm');
    expect(chat.streaming!.content, 'Hello');
    expect(chat.lastSeq, 5);

    chat = play([
      ServerMessageMessageDone(
        session: 'a',
        seq: 6,
        message: const ChatMessage(role: Role.assistant, content: '', thinking: 'hm'),
      ),
      ServerMessageToolCall(session: 'a', seq: 7, callId: 'c1', name: 'run', args: {'site': 'tools', 'recipe': 'hello', 'args': ['bob']}),
      ServerMessageToolResult(session: 'a', seq: 8, callId: 'c1', result: 'hi bob', isError: false),
      ServerMessageMessageDone(session: 'a', seq: 9, message: const ChatMessage(role: Role.assistant, content: 'Done.')),
      ServerMessageTurnDone(session: 'a', seq: 10, cancelled: false),
    ], chat: chat);
    expect(chat.streaming, isNull);
    final items = chat.items;
    expect(items.map((i) => i.runtimeType), [UserItem, ThinkingItem, ToolItem, TextItem]);
    final tool = items[2] as ToolItem;
    expect(tool.summary, 'tools › hello bob');
    expect(tool.result, 'hi bob');
    expect((items[3] as TextItem).text, 'Done.');
  });

  test('replayed and foreign events are skipped', () {
    final chat = play([
      ServerMessageMessageDone(session: 'a', seq: 1, message: const ChatMessage(role: Role.user, content: 'hi')),
      ServerMessageMessageDone(session: 'a', seq: 1, message: const ChatMessage(role: Role.user, content: 'hi')),
      ServerMessageMessageDone(session: 'b', seq: 2, message: const ChatMessage(role: Role.user, content: 'other')),
    ]);
    expect(chat.items.length, 1);
    expect(chat.lastSeq, 1);
  });

  test('messages from apps and workers say so; cancelled turns leave a note', () {
    final chat = play([
      ServerMessageMessageDone(
        session: 'a',
        seq: 1,
        message: const ChatMessage(role: Role.user, content: 'box', source: MessageSourceApp(site: 'design')),
      ),
      ServerMessageMessageDone(
        session: 'a',
        seq: 2,
        message: const ChatMessage(role: Role.user, content: 'done', source: MessageSourceWorker(session: 'b2')),
      ),
      ServerMessageMessageDelta(session: 'a', seq: 3, text: 'par'),
      ServerMessageTurnDone(session: 'a', seq: 4, cancelled: true),
    ]);
    expect((chat.items[0] as UserItem).source, 'design app');
    expect((chat.items[1] as UserItem).source, 'worker #b2');
    expect((chat.items[2] as NoteItem).text, 'cancelled');
    expect(chat.streaming, isNull);
  });

  test('tool arguments on one line', () {
    expect(summarizeArgs({'url': 'site://home'}), 'site://home');
    expect(summarizeArgs(<String, Object?>{}), '');
    expect(summarizeArgs({'x': 1}), '{"x":1}');
  });

  test("Claude Code's tools: what they touch, edits as diffs, sub-agents nested", () {
    const wt = '/home/me/.composer/state/worktrees/0007/src/main.rs';
    expect(toolSummary('Edit', {'file_path': wt}), 'src/main.rs');
    expect(toolSummary('Bash', {'command': 'cargo test\necho'}), 'cargo test');
    expect(toolSummary('Grep', {'pattern': 'fn main', 'path': wt}), 'fn main in src/main.rs');
    expect(toolSummary('Agent', {'description': 'Read a.txt'}), 'Read a.txt');
    expect(toolSummary('TaskUpdate', {'taskId': '1', 'status': 'completed'}), '#1 completed');
    expect(toolSummary('open', {'url': 'site://home'}), 'site://home');
    expect(toolDetail('Edit', {'old_string': 'one', 'new_string': 'two\nthree'}), '- one\n+ two\n+ three');
    expect(
      toolDetail('TodoWrite', {
        'todos': [
          {'content': 'a', 'status': 'completed'},
          {'content': 'b', 'status': 'pending'},
        ],
      }),
      '[x] a\n[ ] b',
    );
    expect(toolDetail('Read', {'file_path': 'x'}), isNull);

    final chat = play([
      const ServerMessageToolCall(session: 'a', seq: 1, callId: 'a1', name: 'Agent', args: {'description': 'Look'}),
      const ServerMessageToolCall(session: 'a', seq: 2, callId: 'r1', name: 'Read', args: {'file_path': 'x'}, parent: 'a1'),
      const ServerMessageToolResult(session: 'a', seq: 3, callId: 'r1', result: 'ok', isError: false),
    ]);
    final read = chat.items[1] as ToolItem;
    expect(((chat.items[0] as ToolItem).nested, read.nested, read.result), (false, true, 'ok'));
  });
}
