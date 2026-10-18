import 'package:composer_app/state/actions.dart';
import 'package:composer_app/state/app_state.dart';
import 'package:composer_app/state/effects.dart';
import 'package:composer_app/state/reduce.dart';
import 'package:composer_app/state/server_entry.dart';
import 'package:composer_app/state/types.dart';
import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter_test/flutter_test.dart';

const home = ServerEntry(url: 'https://home.ts.net', name: 'home', token: 't1');
const work = ServerEntry(url: 'https://work.ts.net', name: 'work', token: 't2');
const info = ServerInfo(name: 'home-pc', version: '0.1.0', model: 'qwen', slots: 2);

SessionInfo session(String id, String updated) =>
    SessionInfo(id: id, agent: 'default', state: TurnState.idle, lastSeq: 0, updated: updated);

AppState loaded(List<ServerEntry> servers) => reduce(AppState.initial, ServersLoaded(servers, StorageKind.keyring)).$1;

void main() {
  test('loading connects every server and shows the first', () {
    final (state, effects) = reduce(AppState.initial, const ServersLoaded([home, work], StorageKind.file));
    expect(state.loaded, isTrue);
    expect(state.active, home.url);
    expect(state.storage, StorageKind.file);
    expect(effects.whereType<Connect>().map((e) => e.server.url), [home.url, work.url]);
  });

  test('pairing adds the server under the name it reports, saves, connects and selects it', () {
    var (state, effects) = reduce(loaded([work]), const PairRequested(url: 'https://home.ts.net', code: 'AB', deviceName: 'pc'));
    expect(state.pairing.busy, isTrue);
    expect(effects.single, isA<Pair>());

    (state, effects) = reduce(state, const PairSucceeded(home, info));
    expect(state.servers.map((s) => s.name), ['work', 'home-pc']);
    expect(state.active, home.url);
    expect(state.pairing.busy, isFalse);
    expect(state.statusOf(home.url).info, info);
    expect(effects.whereType<SaveServers>().single.servers.length, 2);
    expect(effects.whereType<Connect>().single.server.token, 't1');
  });

  test('pairing a known server again replaces its token', () {
    final (state, _) = reduce(loaded([home]), PairSucceeded(ServerEntry(url: home.url, name: 'x', token: 'new'), info));
    expect(state.servers.single.token, 'new');
  });

  test('a failed pairing shows why', () {
    final state = reduce(loaded([]), const PairFailed('that code didn\'t work')).$1;
    expect(state.pairing.error, contains('code'));
    expect(reduce(state, const PairingReset()).$1.pairing.error, isNull);
  });

  test('removing the active server disconnects it and selects another', () {
    final (state, effects) = reduce(loaded([home, work]), ServerRemoved(home.url));
    expect(state.servers, [work]);
    expect(state.active, work.url);
    expect(state.status.containsKey(home.url), isFalse);
    expect(effects.whereType<Disconnect>().single.url, home.url);
  });

  test('link changes and errors', () {
    var state = reduce(loaded([home]), LinkChanged(home.url, LinkState.offline, error: 'lost')).$1;
    expect(state.statusOf(home.url).error, 'lost');
    state = reduce(state, LinkChanged(home.url, LinkState.online)).$1;
    expect(state.statusOf(home.url).link, LinkState.online);
    expect(state.statusOf(home.url).error, isNull);
    // Events for a removed server are dropped.
    final removed = reduce(state, ServerRemoved(home.url)).$1;
    expect(identical(reduce(removed, LinkChanged(home.url, LinkState.online)).$1, removed), isTrue);
  });

  test('a new server name is stored', () {
    final (state, effects) = reduce(loaded([home]), InfoReceived(home.url, info));
    expect(state.servers.single.name, 'home-pc');
    expect(effects.single, isA<SaveServers>());
    expect(reduce(state, InfoReceived(home.url, info)).$2, isEmpty);
  });

  test('sessions: listed newest first, updated in place', () {
    var state = reduce(
      loaded([home]),
      ServerEvent(home.url, ServerMessageOk(id: 1, result: CommandResultSessions(sessions: [session('a', '2026-01-01'), session('b', '2026-02-01')]))),
    ).$1;
    expect(state.statusOf(home.url).sessions.map((s) => s.id), ['b', 'a']);
    state = reduce(state, ServerEvent(home.url, ServerMessageSessionUpdated(session: session('a', '2026-03-01')))).$1;
    expect(state.statusOf(home.url).sessions.map((s) => s.id), ['a', 'b']);
  });

  test('resuming retries only the offline servers', () {
    var state = loaded([home, work]);
    state = reduce(state, LinkChanged(home.url, LinkState.online)).$1;
    state = reduce(state, LinkChanged(work.url, LinkState.offline, error: 'x')).$1;
    expect(reduce(state, const AppResumed()).$2.whereType<Reconnect>().map((e) => e.url), [work.url]);
  });

  test('switching and shortcuts only select known servers', () {
    final state = loaded([home, work]);
    expect(reduce(state, ServerSelected(work.url)).$1.active, work.url);
    expect(reduce(state, const ServerSelected('https://nope')).$1.active, home.url);
  });
}
