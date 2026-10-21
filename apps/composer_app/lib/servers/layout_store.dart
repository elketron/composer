import 'dart:convert';
import 'dart:io';

import 'package:path_provider/path_provider.dart';

import '../state/types.dart';

/// The wide layout (collapsed session list, side pane width) in `layout.json`, beside the
/// server list (`COMPOSER_APP_DATA`, else the app's support directory). Nothing secret.
class LayoutStore {
  static Future<File> _file() async {
    final dataDir = Platform.environment['COMPOSER_APP_DATA'];
    final dir = dataDir != null && dataDir.isNotEmpty ? dataDir : (await getApplicationSupportDirectory()).path;
    return File('$dir/layout.json');
  }

  static Future<LayoutPrefs> load() async {
    try {
      final file = await _file();
      if (!await file.exists()) return const LayoutPrefs();
      return LayoutPrefs.fromJson(jsonDecode(await file.readAsString()) as Map<String, Object?>);
    } catch (_) {
      return const LayoutPrefs();
    }
  }

  static Future<void> save(LayoutPrefs layout) async {
    try {
      final file = await _file();
      await file.parent.create(recursive: true);
      await file.writeAsString(jsonEncode(layout.toJson()));
    } catch (_) {
      // A layout that isn't kept is no reason to fail.
    }
  }
}
