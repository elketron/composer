import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../store_scope.dart';

/// Pick the open session's project from the projects directory, or unset it.
void showProjectSheet(BuildContext context, String url) => showModalBottomSheet<void>(
      context: context,
      showDragHandle: true,
      isScrollControlled: true,
      builder: (sheet) {
        final store = StoreScope.read(context);
        final status = store.state.statusOf(url);
        final current = status.chat == null ? null : status.session(status.chat!.session)?.project;
        void choose(String? project) {
          Navigator.pop(sheet);
          store.dispatch(ProjectChosen(url, project));
        }

        return SafeArea(
          child: ConstrainedBox(
            constraints: BoxConstraints(maxHeight: MediaQuery.sizeOf(context).height * 0.7),
            child: ListView(shrinkWrap: true, padding: const EdgeInsets.fromLTRB(8, 0, 8, 12), children: [
              const Padding(
                padding: EdgeInsets.fromLTRB(8, 0, 8, 8),
                child: Text('Project', style: TextStyle(fontSize: 16, fontWeight: FontWeight.w600)),
              ),
              if (current != null)
                ListTile(
                  key: const Key('unset-project'),
                  leading: const Icon(Icons.folder_off_outlined, size: 20),
                  title: Text('Unset ${current.split('/').last}'),
                  subtitle: const Text('New chats from here start without one', style: TextStyle(fontSize: 12, color: t.faint)),
                  onTap: () => choose(null),
                ),
              for (final p in status.projects)
                ListTile(
                  key: Key('choose-${p.name}'),
                  selected: current?.split('/').last == p.name,
                  leading: const Icon(Icons.folder_outlined, size: 20),
                  title: Text(p.name, style: mono(size: 13, color: t.text)),
                  subtitle: Text(p.path, style: mono(size: 11, color: t.faint)),
                  onTap: () => choose(p.name),
                ),
            ]),
          ),
        );
      },
    );
