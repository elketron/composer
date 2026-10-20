import 'dart:io';

import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';

import '../../servers/server_api.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../store_scope.dart';

/// A file sent with a message: images as a thumbnail, other files by name.
class AttachmentView extends StatelessWidget {
  const AttachmentView({super.key, required this.url, required this.session, required this.attachment});

  final String url;
  final String session;
  final Attachment attachment;

  @override
  Widget build(BuildContext context) {
    final token = StoreScope.of(context).state.servers.where((s) => s.url == url).firstOrNull?.token;
    final file = Container(
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 5),
      decoration: BoxDecoration(color: t.raised, borderRadius: BorderRadius.circular(t.radius)),
      child: Row(mainAxisSize: MainAxisSize.min, children: [
        const Icon(Icons.description_outlined, size: 15, color: t.muted),
        const SizedBox(width: 6),
        Flexible(child: Text(attachment.name, overflow: TextOverflow.ellipsis, style: mono(size: 12, color: t.text))),
        const SizedBox(width: 6),
        Text(_size(attachment.size), style: mono(size: 11, color: t.faint)),
      ]),
    );
    if (!attachment.mime.startsWith('image/') || token == null) return file;
    return ClipRRect(
      borderRadius: BorderRadius.circular(t.radius),
      child: Image.network(
        attachmentUri(url, session, attachment.file).toString(),
        headers: {HttpHeaders.authorizationHeader: 'Bearer $token'},
        width: 220,
        fit: BoxFit.cover,
        semanticLabel: attachment.name,
        errorBuilder: (_, _, _) => file,
      ),
    );
  }
}

String _size(int bytes) => bytes < 1024
    ? '$bytes B'
    : bytes < 1024 * 1024
        ? '${(bytes / 1024).round()} KB'
        : '${(bytes / 1024 / 1024).toStringAsFixed(1)} MB';
