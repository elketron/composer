import 'package:composer_app/ui/format.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('a background-process notice previews as a process line', () {
    expect(previewText('[automatic notice: background process t7d8d371f (`sleep 300`) was stopped by the user]\nNo output.'),
        'process stopped · sleep 300');
    expect(previewText('[automatic notice: background process ta3 (`make -j8`) exited with code 1]\nIts last lines:'),
        'process ended · make -j8');
    expect(previewText('[background process ta33e5a76 (`sleep 120`) was stopped by the user]\nIt printed nothing.'),
        'process stopped · sleep 120');
  });

  test('other previews are as is', () {
    expect(previewText('List the pages of this site'), 'List the pages of this site');
    expect(previewText('[note] something'), '[note] something');
  });
}
