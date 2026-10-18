import 'package:flutter/material.dart';
import 'package:mobile_scanner/mobile_scanner.dart';

import '../theme/tokens.dart' as t;

/// Scans the QR code of `composer pair`; pops with its text.
class QrScanPage extends StatefulWidget {
  const QrScanPage({super.key});

  @override
  State<QrScanPage> createState() => _QrScanPageState();
}

class _QrScanPageState extends State<QrScanPage> {
  bool _done = false;

  @override
  Widget build(BuildContext context) => Scaffold(
        appBar: AppBar(title: const Text('Scan the pairing code')),
        body: Stack(children: [
          MobileScanner(
            onDetect: (capture) {
              final value = capture.barcodes.map((b) => b.rawValue).nonNulls.firstOrNull;
              if (value == null || _done) return;
              _done = true;
              Navigator.of(context).pop(value);
            },
          ),
          Align(
            alignment: Alignment.bottomCenter,
            child: Container(
              margin: const EdgeInsets.all(16),
              padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
              decoration: BoxDecoration(color: t.overlay, borderRadius: BorderRadius.circular(t.radius)),
              child: const Text('Run composer pair on the server and point the camera at its QR code.'),
            ),
          ),
        ]),
      );
}
