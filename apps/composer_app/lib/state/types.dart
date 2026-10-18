// Plain state types (no behaviour).

/// A server's WebSocket.
enum LinkState { connecting, online, offline }

/// Where device tokens are kept.
enum StorageKind {
  /// The platform keyring: Secret Service, Android Keystore, Windows Credential Manager.
  keyring,

  /// A file only you can read (0600), like `composer login` uses: no keyring was available.
  file,
}

/// A short message for a snackbar; `id` tells repeats apart.
class Notice {
  const Notice(this.id, this.text);
  final int id;
  final String text;
}

/// Adding a server.
class PairingState {
  const PairingState({this.busy = false, this.error});

  final bool busy;
  final String? error;
}
