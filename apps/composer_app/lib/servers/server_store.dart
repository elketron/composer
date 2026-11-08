import 'dart:convert';

import 'package:flutter_secure_storage/flutter_secure_storage.dart';

import '../state/server_entry.dart';

/// Keeps the server list (with device tokens) in the Android Keystore.
class ServerStore {
  ServerStore();

  static const _key = 'servers';
  static const _secure = FlutterSecureStorage();

  Future<List<ServerEntry>> load() async {
    final text = await _secure.read(key: _key);
    if (text == null || text.isEmpty) return [];
    return (jsonDecode(text) as List).map((e) => ServerEntry.fromJson(e as Map<String, Object?>)).toList();
  }

  Future<void> save(List<ServerEntry> servers) =>
      _secure.write(key: _key, value: jsonEncode([for (final s in servers) s.toJson()]));
}
