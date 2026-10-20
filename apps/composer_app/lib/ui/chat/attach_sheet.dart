import 'dart:io';

import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:image_picker/image_picker.dart' hide PickedFile;

import '../../state/actions.dart';
import '../../state/types.dart';
import '../../theme/tokens.dart' as t;
import '../store_scope.dart';

/// Photos are shrunk on the phone before they go out (the server does it again, and drops EXIF).
const _maxEdge = 1500.0;
const _quality = 85;

/// Camera (phones), photos or any file; what is picked starts uploading to the open chat.
Future<void> showAttachSheet(BuildContext context, String url) async {
  final store = StoreScope.read(context);
  final phone = Platform.isAndroid || Platform.isIOS;
  final choice = await showModalBottomSheet<String>(
    context: context,
    backgroundColor: t.surface,
    builder: (context) => SafeArea(
      child: Column(mainAxisSize: MainAxisSize.min, children: [
        if (phone)
          ListTile(
            key: const Key('attach-camera'),
            leading: const Icon(Icons.photo_camera_outlined),
            title: const Text('Take a photo'),
            onTap: () => Navigator.pop(context, 'camera'),
          ),
        ListTile(
          key: const Key('attach-photos'),
          leading: const Icon(Icons.photo_library_outlined),
          title: const Text('Photos'),
          onTap: () => Navigator.pop(context, 'photos'),
        ),
        ListTile(
          key: const Key('attach-file'),
          leading: const Icon(Icons.attach_file),
          title: const Text('File'),
          subtitle: const Text('PDF, text, code', style: TextStyle(color: t.muted)),
          onTap: () => Navigator.pop(context, 'file'),
        ),
      ]),
    ),
  );
  final List<PickedFile> files;
  switch (choice) {
    case 'camera':
      final photo = await ImagePicker().pickImage(source: ImageSource.camera, maxWidth: _maxEdge, maxHeight: _maxEdge, imageQuality: _quality);
      files = [if (photo != null) PickedFile(photo.name, path: photo.path)];
    case 'photos':
      final photos = await ImagePicker().pickMultiImage(maxWidth: _maxEdge, maxHeight: _maxEdge, imageQuality: _quality, limit: 5);
      files = [for (final p in photos) PickedFile(p.name, path: p.path)];
    case 'file':
      final picked = await FilePicker.pickFiles();
      files = [
        // Android hands out content:// URIs without a path: read those.
        for (final f in picked) f.path != null ? PickedFile(f.name, path: f.path) : PickedFile(f.name, bytes: await f.readAsBytes()),
      ];
    default:
      return;
  }
  if (files.isNotEmpty) store.dispatch(FilesPicked(url, files));
}
