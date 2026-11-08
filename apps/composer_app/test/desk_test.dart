import 'package:composer_app/state/actions.dart';
import 'package:composer_app/state/app_state.dart';
import 'package:composer_app/state/desk_view.dart';
import 'package:composer_app/state/effects.dart';
import 'package:composer_app/state/reduce.dart';
import 'package:composer_app/state/server_entry.dart';
import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter_test/flutter_test.dart';

const homeUrl = 'https://home.ts.net/w/default';
const home = ServerEntry(url: homeUrl, name: 'home', token: 't');

DeskEntry entry(String id, DeskTab tab, {bool unseen = false}) => DeskEntry(id: id, tab: tab, unseen: unseen);

final page = entry('page:site://plans', const DeskTabPage(url: 'site://plans'), unseen: true);
final app = entry('app:board', const DeskTabApp(site: 'board', project: 'composer'));

/// A server with session `a` open and attached.
AppState opened() {
  var state = reduce(AppState.initial, const ServersLoaded([home])).$1;
  state = reduce(state, const SessionOpened(homeUrl, 'a')).$1;
  return reduce(state, const HistoryLoaded(homeUrl, 'a', null, [])).$1;
}

void main() {
  test('attaching asks for the desk too', () {
    var (state, effects) = reduce(AppState.initial, const ServersLoaded([home]));
    state = reduce(state, const SessionOpened(homeUrl, 'a')).$1;
    (_, effects) = reduce(state, const HistoryLoaded(homeUrl, 'a', null, []));
    expect(effects.whereType<SendCommand>().single.command, isA<ClientCommandSessionAttach>());
    expect(effects.whereType<FetchDesk>().single.session, 'a');
  });

  test('the desk and selection of the open session', () {
    var state = opened();
    final desk = Desk(tabs: [page, app], active: page.id);
    state = reduce(state, DeskLoaded(homeUrl, 'a', desk)).$1;
    expect(state.statusOf(homeUrl).chat!.desk, desk);

    // Another session's desk is not this chat's.
    state = reduce(state, DeskLoaded(homeUrl, 'b', const Desk())).$1;
    expect(state.statusOf(homeUrl).chat!.desk, desk);

    final selection = const Selection(app: 'board', label: 'ticket 0042');
    state = reduce(
      state,
      ServerEvent(homeUrl, ServerMessageSelectionChanged(session: 'a', selection: selection)),
    ).$1;
    expect(state.statusOf(homeUrl).chat!.selection, selection);
    state = reduce(state, ServerEvent(homeUrl, const ServerMessageSelectionChanged(session: 'a'))).$1;
    expect(state.statusOf(homeUrl).chat!.selection, isNull);
  });

  test('a desk.changed event replaces it', () {
    var state = opened();
    final desk = Desk(tabs: [page]);
    state = reduce(
      state,
      ServerEvent(homeUrl, ServerMessageDeskChanged(session: 'a', desk: desk, by: DeskBy.agent, opened: page.id)),
    ).$1;
    expect(state.statusOf(homeUrl).chat!.desk, desk);
  });

  test('opening, closing and focusing send the commands', () {
    var state = opened();
    var (next, effects) = reduce(state, DeskTabOpened(homeUrl, const DeskTabPage(url: 'site://plans')));
    var command = effects.whereType<SendCommand>().single.command as ClientCommandDeskOpen;
    expect(command.session, 'a');
    expect(command.focus, isTrue);
    state = next;

    (_, effects) = reduce(state, const DeskTabClosed(homeUrl, 'page:site://plans'));
    expect((effects.whereType<SendCommand>().single.command as ClientCommandDeskClose).tab, 'page:site://plans');

    (_, effects) = reduce(state, const DeskTabFocused(homeUrl, 'page:site://plans'));
    expect((effects.whereType<SendCommand>().single.command as ClientCommandDeskFocus).tab, 'page:site://plans');
  });

  test('clearing the selection drops the chip and tells the server', () {
    var state = reduce(
      opened(),
      ServerEvent(homeUrl, ServerMessageSelectionChanged(session: 'a', selection: const Selection(app: 'x', label: 'hunk 1'))),
    ).$1;
    final (next, effects) = reduce(state, const SelectionCleared(homeUrl));
    expect(next.statusOf(homeUrl).chat!.selection, isNull);
    expect((effects.whereType<SendCommand>().single.command as ClientCommandSelectionClear).session, 'a');
  });

  test('the phone hides terminals and labels the rest', () {
    final desk = DeskView(Desk(tabs: [
      page,
      entry('terminal:1', const DeskTabTerminal(id: '1')),
      app,
      entry('changes:composer', const DeskTabChanges(project: 'composer')),
    ], active: 'terminal:1'));
    expect(desk.tabs.map((e) => e.id), [page.id, app.id, 'changes:composer']);
    expect(desk.shown?.id, page.id, reason: 'a hidden terminal is not the shown tab');
    expect(desk.unseen, 1);
    expect(DeskView.label(page), 'plans');
    expect(DeskView.label(app), 'board · composer');
    expect(DeskView.label(DeskEntry(id: 'changes:c', tab: const DeskTabChanges(project: 'c'))), 'changes · c');
  });
}
