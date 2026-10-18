import 'package:composer_app/state/site_url.dart';
import 'package:composer_app/ui/pages/page_body.dart';
import 'package:flutter_test/flutter_test.dart';

String show(LinkTarget target) => switch (target) {
      SiteLink(:final url) => url,
      ExternalLink(:final url) => 'ext:$url',
      InvalidLink() => 'invalid',
    };

void main() {
  test('resolves like crates/core SiteUrl::join', () {
    const base = 'site://memory/index.md';
    expect(show(resolveLink('user.md', base)), 'site://memory/user.md');
    expect(show(resolveLink('site://plans/composer', base)), 'site://plans/composer');
    expect(show(resolveLink('https://example.com', base)), 'ext:https://example.com');
    expect(show(resolveLink('#pages', base)), 'site://memory/index.md#pages');
    expect(show(resolveLink('topics/a.md#x', base)), 'site://memory/topics/a.md#x');
    expect(show(resolveLink('../b.md', 'site://memory/topics/a.md')), 'site://memory/b.md');
    expect(show(resolveLink('/top.md', 'site://memory/topics/a.md')), 'site://memory/top.md');
    expect(show(resolveLink('mailto:me@x', base)), 'ext:mailto:me@x');
  });

  test('links may not escape their site', () {
    expect(show(resolveLink('../../etc/passwd', 'site://memory/user.md')), 'invalid');
    expect(show(resolveLink('relative.md', null)), 'invalid');
  });

  test('the page API path', () {
    expect(pageApiPath('site://memory'), '/api/pages/memory');
    expect(pageApiPath('site://memory/user.md#top'), '/api/pages/memory/user.md');
  });

  test('frontmatter is not rendered', () {
    expect(PageBody.body('---\ntitle: X\n---\n# X\n'), '# X\n');
    expect(PageBody.body('# no frontmatter'), '# no frontmatter');
    expect(PageBody.startsWithHeading('\n# Plans\n'), isTrue);
    expect(PageBody.startsWithHeading('Some text\n# Later'), isFalse);
    expect(PageBody.startsWithHeading('## Sub'), isFalse);
  });
}
