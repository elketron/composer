import 'package:flutter/material.dart';

import '../store_scope.dart';
import 'reader.dart';

/// The reader as its own screen (phones).
class ReaderScreen extends StatelessWidget {
  const ReaderScreen({super.key, required this.url});

  final String url;

  @override
  Widget build(BuildContext context) {
    final status = StoreScope.of(context).state.statusOf(url);
    return Scaffold(
      body: SafeArea(child: Reader(url: url, status: status, onClose: () => Navigator.of(context).maybePop())),
    );
  }
}
