import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../../state/types.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../store_scope.dart';

/// The files attached to the message being written: uploading (with progress), ready, or
/// failed (retry); × removes one.
class UploadChips extends StatelessWidget {
  const UploadChips({super.key, required this.url, required this.uploads});

  final String url;
  final List<Upload> uploads;

  @override
  Widget build(BuildContext context) {
    final store = StoreScope.read(context);
    return Padding(
      padding: const EdgeInsets.only(bottom: 8),
      child: Wrap(spacing: 6, runSpacing: 6, children: [
        for (final u in uploads)
          Container(
            key: Key('upload-${u.id}'),
            padding: const EdgeInsets.only(left: 8),
            decoration: BoxDecoration(
              color: t.raised,
              border: Border.all(color: u.status == UploadStatus.failed ? t.err.withValues(alpha: 0.5) : t.border),
              borderRadius: BorderRadius.circular(t.radius),
            ),
            child: Row(mainAxisSize: MainAxisSize.min, children: [
              Icon(_isImage(u.file.name) ? Icons.image_outlined : Icons.description_outlined, size: 15, color: t.muted),
              const SizedBox(width: 6),
              ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: 160),
                child: Tooltip(
                  message: u.error ?? u.file.name,
                  child: Text(u.attachment?.name ?? u.file.name, overflow: TextOverflow.ellipsis, style: mono(size: 12, color: t.text)),
                ),
              ),
              const SizedBox(width: 6),
              switch (u.status) {
                UploadStatus.uploading => SizedBox(
                    width: 14,
                    height: 14,
                    child: CircularProgressIndicator(strokeWidth: 2, value: u.progress > 0 ? u.progress : null),
                  ),
                UploadStatus.ready => const Icon(Icons.check, size: 15, color: t.ok),
                UploadStatus.failed => InkWell(
                    key: Key('retry-${u.id}'),
                    onTap: () => store.dispatch(UploadRetried(url, u.id)),
                    child: const Tooltip(message: 'Retry', child: Icon(Icons.refresh, size: 16, color: t.err)),
                  ),
              },
              IconButton(
                key: Key('remove-${u.id}'),
                tooltip: 'Remove',
                visualDensity: VisualDensity.compact,
                iconSize: 15,
                onPressed: () => store.dispatch(UploadRemoved(url, u.id)),
                icon: const Icon(Icons.close, color: t.muted),
              ),
            ]),
          ),
      ]),
    );
  }
}

bool _isImage(String name) => RegExp(r'\.(png|jpe?g|gif|webp|heic)$', caseSensitive: false).hasMatch(name);
