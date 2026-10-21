import 'package:composer_protocol/composer_protocol.dart';

/// What an approval asks, in one line: the recipe with its args, or the server's title.
String approvalHeadline(ServerMessageApprovalRequest a) => switch (a.kind ?? ApprovalKind.recipe) {
      ApprovalKind.recipe => [a.site, '›', a.recipe, ...a.args].join(' '),
      _ => a.title ?? '',
    };

/// The question above it.
String approvalQuestion(ServerMessageApprovalRequest a) => switch (a.kind ?? ApprovalKind.recipe) {
      ApprovalKind.recipe => 'Run recipe?',
      ApprovalKind.merge => 'Merge?',
      ApprovalKind.tool => 'Allow tool?',
      ApprovalKind.amendment => 'Amend the pipeline?',
    };

/// What approving does.
String approvalNote(ServerMessageApprovalRequest a) => switch (a.kind ?? ApprovalKind.recipe) {
      ApprovalKind.recipe => 'Once approved, this recipe runs without asking until it changes.',
      ApprovalKind.merge => "Merges into the project's checked-out branch, then removes the ticket's worktree and branch.",
      ApprovalKind.tool => a.rule == null
          ? 'Claude Code wants a tool outside its allowlist.'
          : 'Always allows ${a.rule} in this project from now on.',
      ApprovalKind.amendment => 'Learned from your edits at its gate: every work reads the change from then on. Dismiss leaves it as it is.',
    };

/// Whether `always` is an answer to it.
bool approvalCanAlways(ServerMessageApprovalRequest a) => a.kind == ApprovalKind.tool && a.rule != null;
