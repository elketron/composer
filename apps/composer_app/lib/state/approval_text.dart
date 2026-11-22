import 'package:composer_protocol/composer_protocol.dart';

/// A site's `mcp.toml` waiting to start its MCP servers (the body lists them), not a recipe.
bool isMcpServers(ServerMessageApprovalRequest a) => (a.kind ?? ApprovalKind.recipe) == ApprovalKind.recipe && a.recipe == 'mcp.toml';

/// A site's `folder:` waiting to count (its one arg is the folder), not a recipe.
bool isFolder(ServerMessageApprovalRequest a) => (a.kind ?? ApprovalKind.recipe) == ApprovalKind.recipe && a.recipe == 'folder:';

/// What an approval asks, in one line: the recipe with its args, or the server's title.
String approvalHeadline(ServerMessageApprovalRequest a) => switch (a.kind ?? ApprovalKind.recipe) {
      ApprovalKind.recipe when isMcpServers(a) => "Start ${a.site}'s MCP servers",
      ApprovalKind.recipe when isFolder(a) => 'Let ${a.site} work in ${a.args.isEmpty ? 'its folder' : a.args.first}',
      ApprovalKind.recipe => [a.site, '›', a.recipe, ...a.args].join(' '),
      _ => a.title ?? '',
    };

/// The question above it.
String approvalQuestion(ServerMessageApprovalRequest a) => switch (a.kind ?? ApprovalKind.recipe) {
      ApprovalKind.recipe when isMcpServers(a) || isFolder(a) => 'Allow?',
      ApprovalKind.recipe => 'Run recipe?',
      ApprovalKind.tool => 'Allow tool?',
      ApprovalKind.mcp => 'Allow MCP tool?',
    };

/// What approving does.
String approvalNote(ServerMessageApprovalRequest a) => switch (a.kind ?? ApprovalKind.recipe) {
      ApprovalKind.recipe when isMcpServers(a) => 'Once approved, they start without asking until mcp.toml changes.',
      ApprovalKind.recipe when isFolder(a) => 'Once approved, the site works there until its folder: names another folder.',
      ApprovalKind.recipe => 'Once approved, this recipe runs without asking until it changes.',
      ApprovalKind.tool => a.rule == null
          ? 'Claude Code wants a tool outside its allowlist.'
          : 'Always allows ${a.rule} in this folder from now on.',
      ApprovalKind.mcp => a.rule == null
          ? "A call to an MCP server's tool."
          : 'Always allows ${a.rule} from now on, for any agent that lists it.',
    };

/// Whether `always` is an answer to it.
bool approvalCanAlways(ServerMessageApprovalRequest a) =>
    (a.kind == ApprovalKind.tool || a.kind == ApprovalKind.mcp) && a.rule != null;
