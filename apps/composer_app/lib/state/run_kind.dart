import 'package:composer_protocol/composer_protocol.dart';

/// How a run kind is named in the app; a session without one is agentic (the server's default).
String runKindLabel(RunKind? kind) => switch (kind ?? RunKind.agentic) {
      RunKind.singleShot => 'single shot',
      RunKind.workflow => 'workflow',
      RunKind.agentic => 'agentic',
    };

/// A session's label: its run kind, and the outside agent that runs it, if any (`agentic · claude`).
String sessionLabel(SessionInfo session) => [runKindLabel(session.kind), ?session.harness].join(' · ');
