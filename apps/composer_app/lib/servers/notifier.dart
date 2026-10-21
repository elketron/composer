import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter_local_notifications/flutter_local_notifications.dart';

import '../state/effects.dart';
import '../state/types.dart';

/// System notifications: desktop (D-Bus on Linux, toasts on Windows) and Android. Taps and the
/// Approve / Deny buttons come back on [taps].
class Notifier {
  final _plugin = FlutterLocalNotificationsPlugin();
  final _taps = StreamController<NotificationTap>.broadcast();
  late final Future<bool> _ready = _init();

  Stream<NotificationTap> get taps => _taps.stream;

  /// Sets up now (on Android this asks for the permission), not on the first notification.
  void start() => _ready;

  static const _channel = AndroidNotificationChannel(
    'composer',
    'Agents',
    description: 'Approvals, finished turns and workers',
    importance: Importance.high,
  );

  Future<bool> _init() async {
    try {
      final ok = await _plugin.initialize(
        settings: const InitializationSettings(
          android: AndroidInitializationSettings('@mipmap/ic_launcher'),
          linux: LinuxInitializationSettings(defaultActionName: 'Open'),
          windows: WindowsInitializationSettings(
            appName: 'composer',
            appUserModelId: 'dev.composer.app',
            guid: '5f0a1c2e-7c39-4e57-9a37-2d4c3f6e8b11',
          ),
          macOS: DarwinInitializationSettings(),
        ),
        onDidReceiveNotificationResponse: _onResponse,
      );
      if (Platform.isAndroid) {
        final android = _plugin.resolvePlatformSpecificImplementation<AndroidFlutterLocalNotificationsPlugin>();
        await android?.createNotificationChannel(_channel);
        await android?.requestNotificationsPermission();
      }
      if (ok != true) debugPrint('notifications unavailable');
      if (Platform.isAndroid) unawaited(_launchedBy());
      return ok ?? false;
    } catch (error) {
      // No notification service (e.g. Linux without a notification daemon): carry on without.
      debugPrint('notifications unavailable: $error');
      return false;
    }
  }

  /// Android: the app was started by tapping a notification (not implemented on Linux).
  Future<void> _launchedBy() async {
    try {
      final launch = await _plugin.getNotificationAppLaunchDetails();
      final response = launch?.notificationResponse;
      if ((launch?.didNotificationLaunchApp ?? false) && response != null) _onResponse(response);
    } catch (_) {}
  }

  void _onResponse(NotificationResponse response) {
    final payload = response.payload;
    if (payload == null) return;
    final json = jsonDecode(payload) as Map<String, Object?>;
    final action = response.actionId;
    _taps.add(NotificationTap(
      url: json['url'] as String,
      session: json['session'] as String?,
      approval: json['approval'] as String?,
      page: json['page'] as String?,
      app: json['app'] as String?,
      action: action == null || action.isEmpty ? null : action,
    ));
  }

  /// A stable id for a key, so a notification can be replaced or withdrawn.
  static int idOf(String key) {
    var hash = 0x811c9dc5;
    for (final unit in key.codeUnits) {
      hash = ((hash ^ unit) * 0x01000193) & 0x7fffffff;
    }
    return hash;
  }

  Future<void> show(Notify note) async {
    if (!await _ready) return;
    final approval = note.approval != null;
    final details = NotificationDetails(
      android: AndroidNotificationDetails(
        _channel.id,
        _channel.name,
        channelDescription: _channel.description,
        importance: Importance.high,
        priority: Priority.high,
        actions: approval
            ? const [
                AndroidNotificationAction('approve', 'Approve', showsUserInterface: true),
                AndroidNotificationAction('deny', 'Deny', showsUserInterface: true),
              ]
            : null,
      ),
      linux: LinuxNotificationDetails(
        actions: approval
            ? const [LinuxNotificationAction(key: 'approve', label: 'Approve'), LinuxNotificationAction(key: 'deny', label: 'Deny')]
            : const [],
        urgency: approval ? LinuxNotificationUrgency.critical : LinuxNotificationUrgency.normal,
      ),
      windows: WindowsNotificationDetails(
        actions: approval
            ? const [WindowsAction(content: 'Approve', arguments: 'approve'), WindowsAction(content: 'Deny', arguments: 'deny')]
            : const [],
      ),
    );
    final payload = jsonEncode({'url': note.url, 'session': note.session, 'approval': note.approval, 'page': note.page, 'app': note.app});
    try {
      await _plugin.show(id: idOf(note.key), title: note.title, body: note.body, notificationDetails: details, payload: payload);
    } catch (_) {
      // No notification service (a desktop without one): the app shows it all anyway.
    }
  }

  Future<void> cancel(String key) async {
    try {
      if (await _ready) await _plugin.cancel(id: idOf(key));
    } catch (_) {
      // As above.
    }
  }
}
