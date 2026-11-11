import 'package:flutter_secure_storage/flutter_secure_storage.dart';

import '../state/live_view_size.dart';

/// Small per-device preferences, kept beside the server list.
class PreferenceStore {
  PreferenceStore();

  static const _liveViewSize = 'live_view_size';
  static const _secure = FlutterSecureStorage();

  Future<LiveViewSize?> loadLiveViewSize() async => LiveViewSize.parse(await _secure.read(key: _liveViewSize));

  Future<void> saveLiveViewSize(LiveViewSize size) => _secure.write(key: _liveViewSize, value: size.name);
}
