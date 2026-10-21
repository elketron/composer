import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

/// App-wide shortcuts that work wherever focus is (or isn't: after the focused widget went away,
/// key events reach no `Shortcuts` widget). Not while a dialog, sheet or menu is on top; keys
/// without ctrl / alt (other than esc) not while you type in a text field.
class AppShortcuts extends StatefulWidget {
  const AppShortcuts({super.key, required this.bindings, required this.child});

  final Map<ShortcutActivator, VoidCallback> bindings;
  final Widget child;

  @override
  State<AppShortcuts> createState() => _AppShortcutsState();
}

class _AppShortcutsState extends State<AppShortcuts> {
  @override
  void initState() {
    super.initState();
    HardwareKeyboard.instance.addHandler(_onKey);
  }

  @override
  void dispose() {
    HardwareKeyboard.instance.removeHandler(_onKey);
    super.dispose();
  }

  bool _onKey(KeyEvent event) {
    if (event is! KeyDownEvent || !mounted) return false;
    if (!(ModalRoute.of(context)?.isCurrent ?? true)) return false;
    // A plain key is typing while a text field has focus.
    final typing = FocusManager.instance.primaryFocus?.context?.findAncestorWidgetOfExactType<EditableText>() != null;
    final keyboard = HardwareKeyboard.instance;
    final modified = keyboard.isControlPressed || keyboard.isAltPressed || keyboard.isMetaPressed;
    for (final MapEntry(key: activator, value: callback) in widget.bindings.entries) {
      if (typing && !modified && event.logicalKey != LogicalKeyboardKey.escape) continue;
      if (activator.accepts(event, keyboard)) {
        callback();
        return true;
      }
    }
    return false;
  }

  @override
  Widget build(BuildContext context) => widget.child;
}
