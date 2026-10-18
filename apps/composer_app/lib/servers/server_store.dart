import 'dart:convert';
import 'dart:io';

import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:path_provider/path_provider.dart';

import '../state/server_entry.dart';
import '../state/types.dart';

/// Keeps the server list (with device tokens) in the platform keyring, or in a 0600 file when
/// there is none (Linux without a Secret Service). `COMPOSER_APP_DATA=<dir>` forces the file,
/// in that directory: for tests, or a second instance.
class ServerStore {
  ServerStore._(this.kind, this._secure, this._file);

  static const _key = 'servers';

  final StorageKind kind;
  final FlutterSecureStorage? _secure;
  final File? _file;

  static Future<ServerStore> open() async {
    final dataDir = Platform.environment['COMPOSER_APP_DATA'];
    if (dataDir != null && dataDir.isNotEmpty) {
      return ServerStore._(StorageKind.file, null, File('$dataDir/servers.json'));
    }
    const secure = FlutterSecureStorage();
    try {
      await secure.read(key: _key);
      return ServerStore._(StorageKind.keyring, secure, null);
    } catch (_) {
      if (Platform.isAndroid || Platform.isIOS) rethrow;
      final dir = await getApplicationSupportDirectory();
      return ServerStore._(StorageKind.file, null, File('${dir.path}/servers.json'));
    }
  }

  Future<List<ServerEntry>> load() async {
    final text = _secure != null ? await _secure.read(key: _key) : await _readFile();
    if (text == null || text.isEmpty) return [];
    return (jsonDecode(text) as List).map((e) => ServerEntry.fromJson(e as Map<String, Object?>)).toList();
  }

  Future<void> save(List<ServerEntry> servers) async {
    final text = jsonEncode([for (final s in servers) s.toJson()]);
    if (_secure != null) return _secure.write(key: _key, value: text);
    final file = _file!;
    await file.parent.create(recursive: true);
    // Restrict it before the tokens go in.
    if (!await file.exists()) await file.writeAsString('');
    if (!Platform.isWindows) await Process.run('chmod', ['600', file.path]);
    await file.writeAsString(text, flush: true);
  }

  Future<String?> _readFile() async {
    final file = _file!;
    return await file.exists() ? file.readAsString() : null;
  }
}
