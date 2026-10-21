import 'server_entry.dart';
import 'server_status.dart';
import 'types.dart';

const _keep = Object();

class AppState {
  const AppState({
    this.loaded = false,
    this.storage = StorageKind.keyring,
    this.servers = const [],
    this.status = const {},
    this.active,
    this.pairing = const PairingState(),
    this.notice,
    this.foreground = true,
    this.notifiedApprovals = const {},
    this.tab = PhoneTab.chats,
    this.layout = const LayoutPrefs(),
  });

  static const initial = AppState();

  /// The server list has been read from storage.
  final bool loaded;
  final StorageKind storage;

  /// In the order they were added.
  final List<ServerEntry> servers;

  /// By server url.
  final Map<String, ServerStatus> status;

  /// The url of the server being shown.
  final String? active;

  final PairingState pairing;

  /// The latest thing to tell you (a failed command).
  final Notice? notice;

  /// The app is in front (and, on desktop, focused): no notifications for what is on screen.
  final bool foreground;

  /// Approval requests already notified (not again after a reconnect re-sends them).
  final Set<String> notifiedApprovals;

  /// The phone layout's tab.
  final PhoneTab tab;

  /// The wide layout's collapsed session list and side pane width.
  final LayoutPrefs layout;

  ServerEntry? get activeServer => servers.where((s) => s.url == active).firstOrNull;

  ServerStatus statusOf(String url) => status[url] ?? ServerStatus.initial;

  AppState copyWith({
    bool? loaded,
    StorageKind? storage,
    List<ServerEntry>? servers,
    Map<String, ServerStatus>? status,
    Object? active = _keep,
    PairingState? pairing,
    Notice? notice,
    bool? foreground,
    Set<String>? notifiedApprovals,
    PhoneTab? tab,
    LayoutPrefs? layout,
  }) =>
      AppState(
        loaded: loaded ?? this.loaded,
        storage: storage ?? this.storage,
        servers: servers ?? this.servers,
        status: status ?? this.status,
        active: identical(active, _keep) ? this.active : active as String?,
        pairing: pairing ?? this.pairing,
        notice: notice ?? this.notice,
        foreground: foreground ?? this.foreground,
        notifiedApprovals: notifiedApprovals ?? this.notifiedApprovals,
        tab: tab ?? this.tab,
        layout: layout ?? this.layout,
      );
}
