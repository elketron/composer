import 'dart:io';

import 'package:flutter_foreground_task/flutter_foreground_task.dart';

import 'idle_task_handler.dart';

/// Android: a foreground service while there are servers, so the app (and its connections)
/// keeps running in the background and approvals and replies still arrive. The service does no
/// work of its own; the connections live in the app.
class BackgroundKeeper {
  bool _initialized = false;

  Future<void> keep(int servers) async {
    if (!Platform.isAndroid) return;
    if (!_initialized) {
      FlutterForegroundTask.init(
        androidNotificationOptions: AndroidNotificationOptions(
          channelId: 'connection',
          channelName: 'Connection',
          channelDescription: 'Shown while composer stays connected in the background',
          channelImportance: NotificationChannelImportance.LOW,
          priority: NotificationPriority.LOW,
          onlyAlertOnce: true,
        ),
        iosNotificationOptions: const IOSNotificationOptions(showNotification: false),
        foregroundTaskOptions: ForegroundTaskOptions(
          eventAction: ForegroundTaskEventAction.nothing(),
          allowWakeLock: true,
          allowWifiLock: true,
        ),
      );
      _initialized = true;
    }
    final running = await FlutterForegroundTask.isRunningService;
    if (servers == 0) {
      if (running) await FlutterForegroundTask.stopService();
      return;
    }
    final text = servers == 1 ? 'Connected to 1 server' : 'Connected to $servers servers';
    if (running) {
      await FlutterForegroundTask.updateService(notificationTitle: 'composer', notificationText: text);
    } else {
      await FlutterForegroundTask.startService(
        serviceId: 7420,
        serviceTypes: [ForegroundServiceTypes.specialUse],
        notificationTitle: 'composer',
        notificationText: text,
        callback: startIdleTask,
      );
    }
  }
}
