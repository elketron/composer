import 'dart:io';

import 'package:flutter/material.dart';

import '../servers/server_api.dart';
import '../state/actions.dart';
import '../theme/composer_theme.dart';
import '../theme/tokens.dart' as t;
import 'qr_scan_page.dart';
import 'store_scope.dart';

/// Pairs this device with a server: scan the QR of `composer pair`, or paste its link, its
/// `composer login` line, or an address plus the code.
class AddServerPage extends StatefulWidget {
  const AddServerPage({super.key});

  static Future<void> open(BuildContext context) {
    StoreScope.read(context).dispatch(const PairingReset());
    return Navigator.of(context).push(MaterialPageRoute(builder: (_) => const AddServerPage()));
  }

  static final canScan = Platform.isAndroid || Platform.isIOS || Platform.isMacOS;

  @override
  State<AddServerPage> createState() => _AddServerPageState();
}

class _AddServerPageState extends State<AddServerPage> {
  final _url = TextEditingController();
  final _code = TextEditingController();
  final _device = TextEditingController(text: Platform.isAndroid ? 'android' : Platform.localHostname);
  AppStore? _store;
  bool _waiting = false;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final store = StoreScope.read(context);
    if (!identical(store, _store)) {
      _store?.removeListener(_onState);
      _store = store..addListener(_onState);
    }
  }

  @override
  void dispose() {
    _store?.removeListener(_onState);
    _url.dispose();
    _code.dispose();
    _device.dispose();
    super.dispose();
  }

  /// Close once pairing went through.
  void _onState() {
    final pairing = _store!.state.pairing;
    if (_waiting && !pairing.busy) {
      _waiting = false;
      if (pairing.error == null && mounted) Navigator.of(context).pop();
    }
  }

  bool _renaming = false;

  /// What the field holds: an address, and the code if it came with it (a link or a
  /// `composer login` line).
  ({String url, String? code}) get _parsed => parsePairInput(_url.text);

  Future<void> _scan() async {
    final value = await Navigator.of(context).push<String>(MaterialPageRoute(builder: (_) => const QrScanPage()));
    if (value == null || !mounted) return;
    setState(() => _url.text = value.trim());
    if (_parsed.code != null) _submit();
  }

  void _submit() {
    final parsed = _parsed;
    final code = parsed.code ?? _code.text.trim();
    if (parsed.url.isEmpty || code.isEmpty) return;
    _waiting = true;
    StoreScope.read(context).dispatch(PairRequested(url: parsed.url, code: code, deviceName: _device.text.trim()));
  }

  @override
  Widget build(BuildContext context) {
    final pairing = StoreScope.of(context).state.pairing;
    final parsed = _parsed;
    final typed = _url.text.trim().isNotEmpty;
    final ready = parsed.url.isNotEmpty && (parsed.code != null || _code.text.trim().isNotEmpty);
    Widget step(int n, InlineSpan text) => Padding(
          padding: const EdgeInsets.only(bottom: 14),
          child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Container(
              width: 22,
              height: 22,
              alignment: Alignment.center,
              decoration: BoxDecoration(shape: BoxShape.circle, border: Border.all(color: t.borderStrong)),
              child: Text('$n', style: mono(size: 11, color: t.text)),
            ),
            const SizedBox(width: 12),
            Expanded(child: Text.rich(text, style: const TextStyle(color: t.muted, fontSize: 14, height: 1.45))),
          ]),
        );
    return Scaffold(
      appBar: AppBar(title: const Text('Add a server')),
      // Top-aligned: the keyboard opening doesn't move what you are looking at.
      body: Align(
        alignment: Alignment.topCenter,
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 420),
          child: ListView(padding: const EdgeInsets.fromLTRB(20, 24, 20, 20), children: [
            step(1, TextSpan(children: [
              const TextSpan(text: 'On the computer running composer: '),
              TextSpan(text: 'composer pair --name ${_device.text.trim()}', style: mono(size: 13, color: t.text)),
            ])),
            step(
              2,
              TextSpan(
                text: AddServerPage.canScan
                    ? 'Scan the QR code it shows, or paste its link below.'
                    : 'Paste its link, or its composer login line, below.',
              ),
            ),
            if (AddServerPage.canScan) ...[
              OutlinedButton.icon(
                style: OutlinedButton.styleFrom(minimumSize: const Size.fromHeight(44)),
                onPressed: pairing.busy ? null : _scan,
                icon: const Icon(Icons.qr_code_scanner, size: 18),
                label: const Text('Scan QR code'),
              ),
              Padding(
                padding: const EdgeInsets.symmetric(vertical: 14),
                child: Text('OR', textAlign: TextAlign.center, style: mono(size: 10.5, color: t.faint)),
              ),
            ] else
              const SizedBox(height: 6),
            TextField(
              key: const Key('server-url'),
              controller: _url,
              onChanged: (_) => setState(() {}),
              keyboardType: TextInputType.url,
              autocorrect: false,
              onSubmitted: (_) => _submit(),
              style: mono(size: 12.5, color: t.text),
              decoration: const InputDecoration(hintText: 'Link, composer login line, or address'),
            ),
            if (typed) ...[
              const SizedBox(height: 8),
              Row(children: [
                Container(width: 7, height: 7, decoration: BoxDecoration(color: parsed.code != null ? t.ok : t.warn, shape: BoxShape.circle)),
                const SizedBox(width: 8),
                Expanded(
                  child: Text(
                    parsed.code != null ? '${shortHost(parsed.url)} · code ${parsed.code}' : '${shortHost(parsed.url)} · needs the code',
                    key: const Key('pair-parsed'),
                    style: TextStyle(fontSize: 12.5, color: parsed.code != null ? t.ok : t.warn),
                  ),
                ),
              ]),
            ],
            // An address alone: the code goes here.
            if (typed && parsed.code == null) ...[
              const SizedBox(height: 10),
              TextField(
                key: const Key('pair-code'),
                controller: _code,
                autocorrect: false,
                onChanged: (_) => setState(() {}),
                style: mono(size: 13, color: t.text),
                textCapitalization: TextCapitalization.characters,
                onSubmitted: (_) => _submit(),
                decoration: const InputDecoration(labelText: 'Code', hintText: 'AB12-CD34'),
              ),
            ],
            if (!typed)
              const Padding(
                padding: EdgeInsets.only(top: 6),
                child: Text('A link, a composer login … line, or an address and code all work.', style: TextStyle(fontSize: 12, color: t.faint)),
              ),
            const SizedBox(height: 22),
            if (pairing.error != null) ...[
              Text(pairing.error!, key: const Key('pair-error'), style: const TextStyle(color: t.err)),
              const SizedBox(height: 12),
            ],
            FilledButton(
              key: const Key('pair'),
              style: FilledButton.styleFrom(minimumSize: const Size.fromHeight(44)),
              onPressed: pairing.busy || !ready ? null : _submit,
              child: pairing.busy
                  ? const SizedBox(width: 16, height: 16, child: CircularProgressIndicator(strokeWidth: 2))
                  : Text('Pair as “${_device.text.trim()}”'),
            ),
            if (_renaming)
              Padding(
                padding: const EdgeInsets.only(top: 12),
                child: TextField(
                  controller: _device,
                  autocorrect: false,
                  autofocus: true,
                  onChanged: (_) => setState(() {}),
                  onSubmitted: (_) => setState(() => _renaming = false),
                  decoration: const InputDecoration(labelText: 'Device name', helperText: 'How the server lists this device'),
                ),
              )
            else
              TextButton(
                style: TextButton.styleFrom(foregroundColor: t.faint),
                onPressed: () => setState(() => _renaming = true),
                child: const Text('Rename this device'),
              ),
          ]),
        ),
      ),
    );
  }
}

/// `https://my-pc.tail1234.ts.net` → `my-pc.tail1234.ts.net`.
String shortHost(String url) => url.replaceFirst(RegExp(r'^https?://'), '');
