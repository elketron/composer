// The phone route for a chat. `closeOnPop` says whether leaving it closes the chat: a page the
// notification path unwinds to make room for the chat it opened is not one the user closed.

import 'package:flutter/material.dart';

import 'chat_page.dart';

class ChatPageRoute extends MaterialPageRoute<void> {
  ChatPageRoute({required this.url}) : super(builder: (context) => ChatPage(url: url));

  final String url;

  /// False when the app pops this page itself; leaving it by hand still closes the chat.
  bool closeOnPop = true;
}
