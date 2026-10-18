import 'package:flutter_foreground_task/flutter_foreground_task.dart';

/// Entry point of the Android foreground service's task.
@pragma('vm:entry-point')
void startIdleTask() => FlutterForegroundTask.setTaskHandler(IdleTaskHandler());

/// The service's task: nothing to do, it only keeps the process (and its connections) alive.
class IdleTaskHandler extends TaskHandler {
  @override
  Future<void> onStart(DateTime timestamp, TaskStarter starter) async {}

  @override
  void onRepeatEvent(DateTime timestamp) {}

  @override
  Future<void> onDestroy(DateTime timestamp, bool isTimeout) async {}
}
