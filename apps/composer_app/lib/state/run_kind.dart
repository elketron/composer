import 'package:composer_protocol/composer_protocol.dart';

/// How a run kind is named in the app; a session without one is agentic (the server's default).
String runKindLabel(RunKind? kind) => switch (kind ?? RunKind.agentic) {
      RunKind.singleShot => 'single shot',
      RunKind.workflow => 'workflow',
      RunKind.agentic => 'agentic',
    };
