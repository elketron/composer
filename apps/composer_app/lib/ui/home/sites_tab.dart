import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../../state/server_status.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../store_scope.dart';
import 'section_label.dart';
import 'site_tile.dart';

/// All the server's sites, their sessions, and a way to start a chat in one.
class SitesTab extends StatefulWidget {
  const SitesTab({super.key, required this.url, required this.status});

  final String url;
  final ServerStatus status;

  @override
  State<SitesTab> createState() => _SitesTabState();
}

class _SitesTabState extends State<SitesTab> {
  @override
  void initState() {
    super.initState();
    // Sites may have changed since the catalog was fetched.
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) StoreScope.read(context).dispatch(CatalogRequested(widget.url));
    });
  }

  @override
  Widget build(BuildContext context) {
    final sites = widget.status.sites;
    if (sites.isEmpty) {
      return Center(
        child: Padding(
          key: const Key('sites-empty'),
          padding: const EdgeInsets.all(32),
          child: const Text('No sites in this server.', textAlign: TextAlign.center, style: TextStyle(color: t.faint, height: 1.5)),
        ),
      );
    }
    return ListView(
      key: const PageStorageKey('sites'),
      padding: const EdgeInsets.fromLTRB(14, 0, 14, 24),
      children: [
        SectionLabel('Sites', trailing: Text('${sites.length}', style: mono(size: 11, color: t.faint))),
        for (final site in sites)
          Padding(
            padding: const EdgeInsets.only(bottom: 8),
            child: SiteTile(url: widget.url, status: widget.status, site: site),
          ),
      ],
    );
  }
}
