// Spike: can flutter_inappwebview (6.2 beta, WPE WebKit on Linux) host composer's
// micro apps inside the Flutter window? Throwaway; see docs/plan-app.md milestone 1.
//
//   SPIKE_URL='http://127.0.0.1:7498/orchestration/?token=…&session=…' flutter run -d linux
//
// Prints CHECK lines (load, live connection = cookie sign-in, send()) for automation.

import 'dart:async';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_inappwebview/flutter_inappwebview.dart';

// Android has no environment for apps: there the URL comes from --dart-define=SPIKE_URL=….
final initialUrl = Platform.environment['SPIKE_URL'] ??
    const String.fromEnvironment('SPIKE_URL', defaultValue: 'about:blank');
// SPIKE_AUTOSEND=1 drives the board's "new ticket" flow with synthetic keys to test send().
final autoSend = Platform.environment['SPIKE_AUTOSEND'] == '1';
// Quit after this many seconds (0: stay open), for unattended runs.
final quitAfter = int.tryParse(Platform.environment['SPIKE_QUIT_AFTER'] ?? '') ?? 0;

// Composer theme tokens (templates/sdk/theme.css).
const bg = Color(0xFF0C0D0F);
const surface = Color(0xFF121317);
const border = Color(0xFF363A42);
const textColor = Color(0xFFD5D8DE);
const muted = Color(0xFF868C96);
const accent = Color(0xFF8FB3E8);
const warn = Color(0xFFD6B06A);

void main() => runApp(const SpikeApp());

class SpikeApp extends StatelessWidget {
  const SpikeApp({super.key});

  @override
  Widget build(BuildContext context) => MaterialApp(
        debugShowCheckedModeBanner: false,
        theme: ThemeData.dark().copyWith(scaffoldBackgroundColor: bg),
        home: const SpikePage(),
      );
}

class SpikePage extends StatefulWidget {
  const SpikePage({super.key});

  @override
  State<SpikePage> createState() => _SpikePageState();
}

class _SpikePageState extends State<SpikePage> {
  final _url = TextEditingController(text: initialUrl);
  final _log = <String>[];
  InAppWebViewController? _web;
  bool _overlay = true;
  bool _checked = false;

  void log(String line) {
    stdout.writeln(line);
    setState(() {
      _log.add(line);
      if (_log.length > 200) _log.removeAt(0);
    });
  }

  @override
  void initState() {
    super.initState();
    if (quitAfter > 0) Timer(Duration(seconds: quitAfter), () => exit(0));
  }

  Future<void> _afterLoad() async {
    if (_checked) return;
    _checked = true;
    final title = await _web?.evaluateJavascript(source: 'document.title');
    log('CHECK loaded title=$title');
    // The app shell's status dot turns on when its WebSocket is up: the cookie sign-in worked.
    for (var i = 0; i < 20; i++) {
      final on = await _web?.evaluateJavascript(source: "!!document.querySelector('.dot:not(.off)')");
      if (on == true) {
        log('CHECK connected=true after ${i * 250}ms');
        break;
      }
      await Future.delayed(const Duration(milliseconds: 250));
      if (i == 19) log('CHECK connected=false');
    }
    // SPIKE_SNAPSHOT=<file.png>: WebKit's own rendering of the page, independent of the
    // Flutter texture (tells "WPE doesn't render" apart from "the texture doesn't show").
    final snapshot = Platform.environment['SPIKE_SNAPSHOT'];
    if (snapshot != null) {
      await Future.delayed(const Duration(seconds: 1));
      final png = await _web?.takeScreenshot();
      if (png != null) await File(snapshot).writeAsBytes(png);
      log('CHECK snapshot ${png == null ? 'failed' : '${png.length} bytes -> $snapshot'}');
    }
    if (autoSend) {
      await _web?.evaluateJavascript(source: r'''
        (async () => {
          const key = (k) => window.dispatchEvent(new KeyboardEvent('keydown', { key: k }));
          key('n');
          await new Promise(r => setTimeout(r, 300));
          const input = document.querySelector('.ask input');
          input.value = 'Spike: embedded webview send() test';
          input.dispatchEvent(new Event('input', { bubbles: true }));
          key('Enter');
        })();
      ''');
      await Future.delayed(const Duration(seconds: 2));
      final notice = await _web?.evaluateJavascript(source: "document.querySelector('.notice')?.textContent ?? ''");
      log('CHECK send notice="$notice"');
    }
  }

  @override
  Widget build(BuildContext context) {
    final mono = const TextStyle(fontFamily: 'monospace', fontSize: 12, color: muted);
    return Scaffold(
      body: Column(children: [
        Container(
          color: surface,
          padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
          child: Row(children: [
            const Text('spike', style: TextStyle(color: textColor, fontWeight: FontWeight.w600)),
            const SizedBox(width: 12),
            Expanded(
              child: TextField(
                controller: _url,
                style: const TextStyle(color: textColor, fontSize: 13),
                decoration: const InputDecoration(
                  isDense: true,
                  border: OutlineInputBorder(borderSide: BorderSide(color: border)),
                ),
                onSubmitted: (url) => _web?.loadUrl(urlRequest: URLRequest(url: WebUri(url))),
              ),
            ),
            const SizedBox(width: 8),
            TextButton(
              onPressed: () => setState(() => _overlay = !_overlay),
              child: Text(_overlay ? 'hide overlay' : 'show overlay', style: const TextStyle(color: accent)),
            ),
          ]),
        ),
        Expanded(
          child: Stack(children: [
            InAppWebView(
              initialUrlRequest: URLRequest(url: WebUri(initialUrl)),
              initialSettings: InAppWebViewSettings(isInspectable: true, transparentBackground: false),
              onWebViewCreated: (controller) {
                _web = controller;
                log('EVENT created');
                // Linux beta: initialUrlRequest can be ignored (#2903), so load explicitly.
                if (Platform.isLinux) controller.loadUrl(urlRequest: URLRequest(url: WebUri(initialUrl)));
              },
              onLoadStart: (_, url) => log('EVENT load start $url'),
              onLoadStop: (_, url) {
                log('EVENT load stop $url');
                _afterLoad();
              },
              onConsoleMessage: (_, message) => log('CONSOLE ${message.messageLevel} ${message.message}'),
              onReceivedError: (_, request, error) => log('ERROR ${request.url}: ${error.description}'),
              onReceivedHttpError: (_, request, response) => log('HTTP ${response.statusCode} ${request.url}'),
            ),
            // Drawn by Flutter on top of the web content: only possible if the webview is a texture.
            if (_overlay)
              Positioned(
                right: 16,
                bottom: 16,
                child: Container(
                  padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
                  decoration: BoxDecoration(
                    color: warn.withValues(alpha: 0.9),
                    borderRadius: BorderRadius.circular(5),
                  ),
                  child: const Text('flutter overlay', style: TextStyle(color: bg, fontWeight: FontWeight.w600)),
                ),
              ),
          ]),
        ),
        Container(
          height: 140,
          color: surface,
          padding: const EdgeInsets.all(8),
          child: ListView(reverse: true, children: [for (final line in _log.reversed) Text(line, style: mono)]),
        ),
      ]),
    );
  }
}
