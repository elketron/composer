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
  int _urlLength = 0;

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

  /// A pasted link or `composer login` line fills both fields. Only for text arriving at once:
  /// while typing, the line looks complete as soon as the code's first letter is in.
  void _onUrlChanged(String text) {
    final pasted = text.length - _urlLength > 1;
    _urlLength = text.length;
    if (pasted) _split();
  }

  void _split() {
    final parsed = parsePairInput(_url.text);
    if (parsed.code == null) return;
    _url.text = parsed.url;
    _code.text = parsed.code!;
    _urlLength = _url.text.length;
  }

  Future<void> _scan() async {
    final value = await Navigator.of(context).push<String>(MaterialPageRoute(builder: (_) => const QrScanPage()));
    if (value == null || !mounted) return;
    final parsed = parsePairInput(value);
    _url.text = parsed.url;
    _code.text = parsed.code ?? '';
    if (parsed.code != null) _submit();
  }

  void _submit() {
    _split();
    final url = parsePairInput(_url.text).url;
    if (url.isEmpty || _code.text.trim().isEmpty) return;
    _waiting = true;
    StoreScope.read(context).dispatch(PairRequested(url: url, code: _code.text.trim(), deviceName: _device.text.trim()));
  }

  @override
  Widget build(BuildContext context) {
    final pairing = StoreScope.of(context).state.pairing;
    return Scaffold(
      appBar: AppBar(title: const Text('Add server')),
      body: Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 420),
          child: ListView(padding: const EdgeInsets.all(20), shrinkWrap: true, children: [
            Text.rich(
              TextSpan(children: [
                const TextSpan(text: 'On the server, run '),
                TextSpan(text: 'composer pair --name <device>', style: mono(size: 13, color: t.text)),
                TextSpan(
                  text: AddServerPage.canScan
                      ? ' and scan its QR code, or paste its link here.'
                      : ' and paste its link, or its composer login line, here.',
                ),
              ]),
              style: const TextStyle(color: t.muted, height: 1.5),
            ),
            const SizedBox(height: 20),
            if (AddServerPage.canScan) ...[
              FilledButton.icon(
                onPressed: pairing.busy ? null : _scan,
                icon: const Icon(Icons.qr_code_scanner, size: 18),
                label: const Text('Scan QR code'),
              ),
              const SizedBox(height: 20),
            ],
            TextField(
              key: const Key('server-url'),
              controller: _url,
              onChanged: _onUrlChanged,
              keyboardType: TextInputType.url,
              autocorrect: false,
              onSubmitted: (_) => _submit(),
              style: mono(size: 13, color: t.text),
              decoration: const InputDecoration(labelText: 'Server', hintText: 'https://home.tail1234.ts.net'),
            ),
            const SizedBox(height: 12),
            TextField(
              key: const Key('pair-code'),
              controller: _code,
              autocorrect: false,
              style: mono(size: 13, color: t.text),
              textCapitalization: TextCapitalization.characters,
              onSubmitted: (_) => _submit(),
              decoration: const InputDecoration(labelText: 'Code', hintText: 'AB12-CD34'),
            ),
            const SizedBox(height: 12),
            TextField(
              controller: _device,
              autocorrect: false,
              onSubmitted: (_) => _submit(),
              decoration: const InputDecoration(labelText: 'Device name', helperText: 'How the server lists this device'),
            ),
            const SizedBox(height: 20),
            if (pairing.error != null) ...[
              Text(pairing.error!, key: const Key('pair-error'), style: const TextStyle(color: t.err)),
              const SizedBox(height: 12),
            ],
            (AddServerPage.canScan ? OutlinedButton.new : FilledButton.new)(
              key: const Key('pair'),
              onPressed: pairing.busy ? null : _submit,
              child: pairing.busy
                  ? const SizedBox(width: 16, height: 16, child: CircularProgressIndicator(strokeWidth: 2))
                  : const Text('Pair'),
            ),
          ]),
        ),
      ),
    );
  }
}
