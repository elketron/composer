// `site://` URLs, as crates/core resolves them (SiteUrl::join).

/// What a link in a page or the chat points at.
sealed class LinkTarget {
  const LinkTarget();
}

final class SiteLink extends LinkTarget {
  const SiteLink(this.url);
  final String url;
}

final class ExternalLink extends LinkTarget {
  const ExternalLink(this.url);
  final String url;
}

final class InvalidLink extends LinkTarget {
  const InvalidLink(this.href);
  final String href;
}

const _scheme = 'site://';

/// Resolves `href` against the page at `base` (a `site://` URL, or null in the chat).
LinkTarget resolveLink(String href, String? base) {
  final scheme = RegExp(r'^([A-Za-z][A-Za-z0-9+.\-]*):').firstMatch(href)?.group(1);
  if (scheme != null && scheme != 'site') return ExternalLink(href);
  if (href.startsWith(_scheme)) return SiteLink(href);
  if (base == null || !base.startsWith(_scheme)) return InvalidLink(href);

  final hash = href.indexOf('#');
  final rel = hash < 0 ? href : href.substring(0, hash);
  final fragment = hash < 0 ? '' : href.substring(hash);
  final (site, path) = splitSiteUrl(withoutFragment(base));
  if (rel.isEmpty) return SiteLink('$_scheme$site${path.isEmpty ? '' : '/$path'}$fragment');

  final parts = rel.startsWith('/') ? <String>[] : path.split('/').where((s) => s.isNotEmpty).toList();
  if (parts.isNotEmpty && !rel.startsWith('/')) parts.removeLast(); // the page's directory
  for (final segment in rel.split('/')) {
    if (segment.isEmpty || segment == '.') continue;
    if (segment == '..') {
      if (parts.isEmpty) return InvalidLink(href);
      parts.removeLast();
    } else {
      parts.add(segment);
    }
  }
  return SiteLink('$_scheme$site${parts.isEmpty ? '' : '/${parts.join('/')}'}$fragment');
}

/// `site://memory/user.md` → (`memory`, `user.md`).
(String, String) splitSiteUrl(String url) {
  final rest = url.startsWith(_scheme) ? url.substring(_scheme.length) : url;
  final slash = rest.indexOf('/');
  return slash < 0 ? (rest, '') : (rest.substring(0, slash), rest.substring(slash + 1));
}

String withoutFragment(String url) => url.split('#').first;

/// The same page, ignoring fragments.
bool samePage(String a, String b) => withoutFragment(a) == withoutFragment(b);

/// `site://memory/user.md` → `/api/pages/memory/user.md`.
String pageApiPath(String url) {
  final (site, path) = splitSiteUrl(withoutFragment(url));
  return path.isEmpty ? '/api/pages/$site' : '/api/pages/$site/$path';
}
