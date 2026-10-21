// HTTP calls to a server, and reading what people paste or scan when adding one.

import 'dart:convert';
import 'dart:io';

import 'package:composer_protocol/composer_protocol.dart';
import 'package:http/http.dart' as http;

import '../state/site_url.dart';
import '../state/types.dart';

const _timeout = Duration(seconds: 10);

/// What a QR code or pasted text says: `https://host/pair?code=AB12-CD34` (the QR of
/// `composer pair`), `composer login https://host AB12-CD34`, or just a server address.
({String url, String? code}) parsePairInput(String input) {
  final words = input.trim().split(RegExp(r'\s+'));
  if (words.length >= 4 && words[0] == 'composer' && words[1] == 'login') {
    return (url: normalizeUrl(words[2]), code: words[3]);
  }
  // An address and the code beside it: `10.0.2.2:7420 AB12-CD34`.
  if (words.length == 2 && RegExp(r'^[A-Za-z0-9]{4}-[A-Za-z0-9]{4}$').hasMatch(words[1])) {
    return (url: normalizeUrl(words[0]), code: words[1]);
  }
  final text = words.join();
  final uri = Uri.tryParse(text.contains('://') ? text : 'https://$text');
  final code = uri?.queryParameters['code'];
  if (uri != null && uri.path.endsWith('/pair') && code != null && code.isNotEmpty) {
    return (url: normalizeUrl(uri.replace(path: uri.path.substring(0, uri.path.length - 5), query: '').toString()), code: code);
  }
  return (url: normalizeUrl(text), code: null);
}

/// `host.ts.net` → `https://host.ts.net`; localhost without a scheme gets http. No trailing slash or query.
String normalizeUrl(String input) {
  var text = input.trim();
  if (text.isEmpty) return text;
  if (!text.contains('://')) {
    final local = text.startsWith('localhost') || text.startsWith('127.') || text.startsWith('[::1]');
    text = '${local ? 'http' : 'https'}://$text';
  }
  // Half typed (`host:78a`) isn't an address yet: kept as typed, never thrown.
  final uri = Uri.tryParse(text);
  if (uri == null) return text;
  final base = uri.replace(query: '', fragment: '').toString().replaceAll(RegExp(r'[?#]+$'), '');
  return base.replaceAll(RegExp(r'/+$'), '');
}

/// The WebSocket address of a server.
Uri wsUri(String url) {
  final uri = Uri.parse(url);
  return uri.replace(scheme: uri.scheme == 'https' ? 'wss' : 'ws', path: '${uri.path}/ws');
}

/// `GET /api/info`; also proves the token is still accepted.
Future<ServerInfo> fetchInfo(String url, String token) async {
  final response = await http
      .get(Uri.parse('$url/api/info'), headers: {HttpHeaders.authorizationHeader: 'Bearer $token'})
      .timeout(_timeout);
  if (response.statusCode == 401) throw const ServerError('this device was revoked (pair it again)');
  if (response.statusCode != 200) throw ServerError('server answered ${response.statusCode}');
  return ServerInfo.fromJson(jsonDecode(response.body) as Map<String, Object?>);
}

/// `GET /api/pages/...` for a `site://` URL.
Future<PageResponse> fetchPage(String url, String token, String page) async {
  final response = await http
      .get(Uri.parse('$url${pageApiPath(page)}'), headers: {HttpHeaders.authorizationHeader: 'Bearer $token'})
      .timeout(_timeout);
  if (response.statusCode == 404) throw const ServerError('no such page');
  if (response.statusCode != 200) throw ServerError('server answered ${response.statusCode}');
  return PageResponse.fromJson(jsonDecode(utf8.decode(response.bodyBytes)) as Map<String, Object?>);
}

/// `GET /api/sites`.
Future<List<SiteSummary>> fetchSites(String url, String token) async {
  final response = await http
      .get(Uri.parse('$url/api/sites'), headers: {HttpHeaders.authorizationHeader: 'Bearer $token'})
      .timeout(_timeout);
  if (response.statusCode != 200) throw ServerError('server answered ${response.statusCode}');
  return (jsonDecode(utf8.decode(response.bodyBytes)) as List)
      .map((e) => SiteSummary.fromJson(e as Map<String, Object?>))
      .toList();
}

/// `GET /api/agents`; empty from an older server.
Future<List<AgentSummary>> fetchAgents(String url, String token) async =>
    [for (final e in await _getList(url, token, '/api/agents')) AgentSummary.fromJson(e as Map<String, Object?>)];

/// `GET /api/projects`; empty from an older server.
Future<List<ProjectSummary>> fetchProjects(String url, String token) async =>
    [for (final e in await _getList(url, token, '/api/projects')) ProjectSummary.fromJson(e as Map<String, Object?>)];

/// `GET /api/search?q=`: full text and `key:value` filters, as the agents' `search`.
Future<List<SearchHit>> searchPages(String url, String token, String query) async => [
      for (final e in await _getList(url, token, '/api/search?q=${Uri.encodeQueryComponent(query)}&limit=40'))
        SearchHit.fromJson(e as Map<String, Object?>),
    ];

Future<List<Object?>> _getList(String url, String token, String path) async {
  final response = await http.get(Uri.parse('$url$path'), headers: {HttpHeaders.authorizationHeader: 'Bearer $token'}).timeout(_timeout);
  if (response.statusCode == 404) return const [];
  if (response.statusCode != 200) throw ServerError('server answered ${response.statusCode}');
  return jsonDecode(utf8.decode(response.bodyBytes)) as List;
}

/// Trades a pairing code for a device token (`POST /api/pair`).
Future<String> redeemCode(String url, String code, String deviceName) async {
  final response = await http
      .post(
        Uri.parse('$url/api/pair'),
        headers: {HttpHeaders.contentTypeHeader: 'application/json'},
        body: jsonEncode({'code': code.trim(), 'name': deviceName}),
      )
      .timeout(_timeout);
  if (response.statusCode == 401) {
    throw const ServerError('that code didn\'t work: it is wrong, used, or older than 10 minutes');
  }
  if (response.statusCode == 404) throw const ServerError('no composer server at that address');
  if (response.statusCode != 200) throw ServerError('server answered ${response.statusCode}');
  return (jsonDecode(response.body) as Map<String, Object?>)['token'] as String;
}

/// A failure worth showing as is.
class ServerError implements Exception {
  const ServerError(this.message);
  final String message;

  @override
  String toString() => message;
}

/// A short reason for a failed request, for the UI.
String describeError(Object error) => switch (error) {
      ServerError(:final message) => message,
      SocketException(:final osError) => 'can\'t reach the server${osError == null ? '' : ' (${osError.message})'}',
      HandshakeException() => 'TLS handshake failed',
      http.ClientException(:final message) => message,
      FormatException() => 'not a composer server (unexpected answer)',
      _ when error.toString().contains('TimeoutException') => 'the server didn\'t answer in time',
      _ => error.toString(),
    };

/// `POST /api/sessions/{session}/attachments` with one file; `onProgress` gets 0–1 as it is
/// sent. Returns what the server stored.
Future<Attachment> uploadAttachment(
  String url,
  String token,
  String session,
  PickedFile file,
  void Function(double progress) onProgress,
) async {
  final request = http.MultipartRequest('POST', Uri.parse('$url/api/sessions/$session/attachments'));
  request.files.add(file.bytes != null
      ? http.MultipartFile.fromBytes('file', file.bytes!, filename: file.name)
      : await http.MultipartFile.fromPath('file', file.path!, filename: file.name));
  // Counted on the way out, for the progress bar.
  final body = request.finalize();
  final total = request.contentLength;
  var sent = 0;
  final streamed = http.StreamedRequest('POST', request.url)
    ..headers.addAll(request.headers)
    ..headers[HttpHeaders.authorizationHeader] = 'Bearer $token'
    ..contentLength = total;
  body.listen(
    (chunk) {
      sent += chunk.length;
      if (total > 0) onProgress(sent / total);
      streamed.sink.add(chunk);
    },
    onDone: streamed.sink.close,
    onError: streamed.sink.addError,
  );
  final response = await http.Response.fromStream(await streamed.send().timeout(const Duration(minutes: 5)));
  if (response.statusCode != 200) {
    String? error;
    try {
      error = (jsonDecode(response.body) as Map<String, Object?>)['error'] as String?;
    } catch (_) {}
    throw ServerError(error ?? 'server answered ${response.statusCode}');
  }
  final stored = (jsonDecode(response.body) as List<Object?>).cast<Map<String, Object?>>();
  return Attachment.fromJson(stored.last);
}

/// The URL of a session's stored attachment (needs the token header).
Uri attachmentUri(String url, String session, String file) => Uri.parse('$url/api/sessions/$session/attachments/${Uri.encodeComponent(file)}');
