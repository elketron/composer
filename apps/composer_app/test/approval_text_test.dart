import 'package:composer_app/state/approval_text.dart';
import 'package:composer_app/state/run_kind.dart';
import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter_test/flutter_test.dart';

ServerMessageApprovalRequest recipe(String name) => ServerMessageApprovalRequest(
    request: 'r', session: 'a', kind: ApprovalKind.recipe, site: 'shop', recipe: name, args: const ['x'], body: 'b', hash: 'h');

void main() {
  test("a site's mcp.toml asks to start its MCP servers, not to run a recipe", () {
    expect(approvalHeadline(recipe('mcp.toml')), "Start shop's MCP servers");
    expect(approvalQuestion(recipe('mcp.toml')), isNot(contains('recipe')));
    expect(approvalHeadline(recipe('build')), 'shop › build x');
    expect(approvalQuestion(recipe('build')), 'Run recipe?');
  });

  test("a site's folder: asks to let the site work there, naming the folder", () {
    expect(approvalHeadline(recipe('folder:')), 'Let shop work in x');
    expect(approvalQuestion(recipe('folder:')), 'Allow?');
    expect(approvalNote(recipe('folder:')), contains('folder:'));
  });

  test('a session label names its harness when it has one', () {
    const session = SessionInfo(id: 'a', state: TurnState.idle, lastSeq: 0, updated: '');
    expect(sessionLabel(session), 'agentic');
    expect(sessionLabel(const SessionInfo(id: 'a', state: TurnState.idle, lastSeq: 0, updated: '', harness: 'claude')), 'agentic · claude');
  });
}
