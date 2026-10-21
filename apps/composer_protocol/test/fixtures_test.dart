// Every sample message Rust writes (test/fixtures, from the generator) must parse in Dart
// and serialize back to the same JSON, so the two sides can't drift apart.

import 'dart:convert';
import 'dart:io';

import 'package:composer_protocol/composer_protocol.dart';
import 'package:test/test.dart';

List<Map<String, Object?>> fixture(String name) =>
    (jsonDecode(File('test/fixtures/$name.json').readAsStringSync()) as List).cast<Map<String, Object?>>();

/// Missing, null and empty lists mean the same on both sides; compare without them.
Object? normalize(Object? value) => switch (value) {
      Map() => {
          for (final entry in value.entries)
            if (entry.value != null && !(entry.value is List && (entry.value as List).isEmpty))
              entry.key: normalize(entry.value),
        },
      List() => value.map(normalize).toList(),
      _ => value,
    };

void roundTrip<T>(Map<String, Object?> json, T Function(Map<String, Object?>) parse, Map<String, Object?> Function(T) dump) {
  final parsed = parse(json);
  expect(normalize(dump(parsed)), normalize(json), reason: 'round trip of $json');
}

void main() {
  test('every server message', () {
    final messages = fixture('ServerMessage');
    for (final json in messages) {
      final message = ServerMessage.fromJson(json);
      expect(message, isNot(isA<ServerMessageUnknown>()), reason: 'unknown: $json');
      roundTrip(json, ServerMessage.fromJson, (m) => m.toJson());
    }
    expect(messages.length, greaterThan(20));
  });

  test('every client request', () {
    for (final json in fixture('ClientRequest')) {
      final request = ClientRequest.fromJson(json);
      expect(request.command, isNot(isA<ClientCommandUnknown>()), reason: 'unknown: $json');
      roundTrip(json, ClientRequest.fromJson, (r) => r.toJson());
    }
  });

  test('http bodies', () {
    final [info, site, page, diff, agent, project, overview] = fixture('http');
    roundTrip(info, ServerInfo.fromJson, (v) => v.toJson());
    roundTrip(site, SiteSummary.fromJson, (v) => v.toJson());
    roundTrip(page, PageResponse.fromJson, (v) => v.toJson());
    roundTrip(diff, ProjectDiff.fromJson, (v) => v.toJson());
    roundTrip(agent, AgentSummary.fromJson, (v) => v.toJson());
    roundTrip(project, ProjectSummary.fromJson, (v) => v.toJson());
    roundTrip(overview, ProjectOverview.fromJson, (v) => v.toJson());
  });

  test('typed access and exhaustive switches', () {
    final event = ServerMessage.fromJson({'type': 'message.done', 'session': 'a', 'seq': 3, 'message': {'role': 'assistant', 'content': 'hi'}});
    final text = switch (event) {
      ServerMessageMessageDone(:final message) => message.content,
      _ => null,
    };
    expect(text, 'hi');
    expect((event as ServerMessageMessageDone).message.toolCalls, isEmpty);
    expect(TurnState.fromJson('awaiting_approval'), TurnState.awaitingApproval);
  });

  test('unknown variants from a newer server are kept, not fatal', () {
    final event = ServerMessage.fromJson({'type': 'something.new', 'x': 1});
    expect(event, isA<ServerMessageUnknown>());
    expect(event.toJson(), {'type': 'something.new', 'x': 1});
  });

  test('commands serialize the way the server expects', () {
    final request = ClientRequest(id: 7, command: ClientCommandMessageSend(session: 'a', text: 'hi'));
    expect(request.toJson(), {'id': 7, 'type': 'message.send', 'session': 'a', 'text': 'hi'});
  });
}
