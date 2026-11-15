#!/usr/bin/env python3
"""Static checks for the desktop and phone surface. Standard library only."""
from html.parser import HTMLParser
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]
SCREENS = sorted((ROOT / "screens").glob("*.html"))
assert len(SCREENS) == 12, "Update the inventory/checks when adding a screen"
DESKTOP = [file for file in SCREENS if "-phone-" not in file.name]
# The session screen and its two overlay states share one shell; the review state is the same
# two panes with another session; home and projects are workspace screens outside a session.
SESSION_STATES = [file for file in DESKTOP if file.name[:2] in ("01", "02", "03")]
WORKSPACE_SCREENS = [file for file in DESKTOP if "-home" in file.name or "-projects" in file.name]
PHONE = [file for file in SCREENS if "-phone-" in file.name]


class Links(HTMLParser):
    def __init__(self):
        super().__init__()
        self.links = []
        self.ids = []

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        for attr in ("href", "src", "data-href"):
            if attrs.get(attr):
                self.links.append(attrs[attr])
        if attrs.get("id"):
            self.ids.append(attrs["id"])


index = (ROOT / "index.html").read_text()
shells = []
for file in [ROOT / "index.html", *SCREENS]:
    text = file.read_text()
    parser = Links()
    parser.feed(text)
    assert len(parser.ids) == len(set(parser.ids)), f"Duplicate ids: {file.name}"
    for link in parser.links:
        assert not re.match(r"(?:https?:)?//", link), f"Remote resource: {link}"
        target = link.split("#")[0]
        assert not target or (file.parent / target).exists(), f"Missing link: {file}: {link}"
    styles = "\n".join(re.findall(r"<style>(.*?)</style>", text, flags=re.S))
    assert not re.search(r"#[0-9a-fA-F]{3,8}\b|\b(?:rgb|hsl)a?\(", styles), f"Raw color: {file}"
    if file in SCREENS:
        assert f'screens/{file.name}' in index, f"Not in inventory: {file.name}"
        if file in PHONE:
            assert text.index('href="../tokens.css"') < text.index('href="../phone-tokens.css"'), "Phone roles are filled from Noema"
        else:
            assert 'href="../tokens.css"' in text and 'href="../base.css"' in text
        assert "workflow" not in text.lower(), "Sites, not workflows"
    if file in DESKTOP:
        assert text.count('class="statusbar"') == 1
        assert text.count('class="titlebar"') == 1
    if file in WORKSPACE_SCREENS:
        assert 'class="home"' in text and 'class="home-tabs"' in text
        assert 'class="chat"' not in text, "Workspace screens are outside the session"
    if file in DESKTOP and file not in WORKSPACE_SCREENS:
        assert '<section class="chat"' in text and '<section class="stage"' in text, "Keep the two-pane session screen"
        assert text.count('class="tool"') == 2
        assert text.count('class="thread"') == 1
        assert text.count('class="approval"') == 1
        assert 'href="11-desktop-home.html"' in text, "The workspace name leads home"
    if file in SESSION_STATES:
        shell = text.split('<main class="workbench"', 1)[1].split("</main>", 1)[0]
        shell = re.sub(r"<!-- overlay-slot -->.*?(?=    </section>\n    <section class=\"stage\")", "", shell, flags=re.S)
        shells.append(shell)
    if file in PHONE:
        assert 'href="../phone.css"' in text
        assert text.count('class="p-status"') == 1
        assert text.count('class="p-header"') == 1
        assert text.count('class="phone-shell"') == 1
        assert "<kbd>" not in text and "NORMAL" not in text
        assert ">Drive<" not in text
        if "inbox" in file.name:
            assert text.count('class="p-session"') == 4
            assert not re.search(r'<h[1-6][^>]*>\s*inbox\s*</h[1-6]>', text, re.I), "Bottom tab already identifies Inbox"
        else:
            assert "p-thread" not in text, "The phone has no threads"
            assert text.count('class="p-approval"') == 1
            assert '3d › export ship.glb' in text, "A recipe approval, as the app asks"
            assert text.count('class="p-message') == 7
    print(f"ok links, palette, content: {file.relative_to(ROOT)}")
assert all(shell == shells[0] for shell in shells), "Shared workbench shells have drifted"

phone_shells = []
for file in PHONE:
    if "inbox" in file.name:
        continue
    text = file.read_text()
    shell = text.split('<div class="p-app"', 1)[1].split('<!-- sheet-slot -->', 1)[0]
    shell = shell.removeprefix(' inert')
    shell = shell.removeprefix(' data-chat-hidden="true"')
    shell = re.sub(r'<!-- live-slot-start -->.*?<!-- live-slot-end -->', '', shell, flags=re.S)
    phone_shells.append(shell)
assert all(shell == phone_shells[0] for shell in phone_shells), "Phone conversation shell drift"

for file in [ROOT / "base.css", ROOT / "phone.css", ROOT / "tokens.css", ROOT / "phone-tokens.css"]:
    text = file.read_text()
    for link in re.findall(r'url\([\'"]?([^\)\'\"]+)', text):
        assert link.startswith("data:") or (file.parent / link).exists(), f"Missing asset: {link}"
    if not file.name.endswith("tokens.css"):
        assert not re.search(r"#[0-9a-fA-F]{3,8}\b|\b(?:rgb|hsl)a?\(", text), "Raw CSS color"

tokens = dict(re.findall(r"(--noema-[\w-]+):\s*([^;]+);", (ROOT / "tokens.css").read_text()))
expected = {
    "bg": "#0e0e0f", "bg-deep": "#0a0a0b", "panel": "#111112", "panel-2": "#131315",
    "surface": "#171718", "surface-2": "#1a1a1c", "border": "#222224", "border-2": "#2a2a2c",
    "fg": "#c2c0b6", "fg-bright": "#d8d6cc", "fg-dim": "#8a8880", "fg-faint": "#6a6865",
    "accent": "#4e46a0", "accent-hi": "#5e56b0", "accent-soft": "#8a82cc",
    "accent-deep": "#12111a", "accent-border": "#2a2740", "ok": "#4CAF82",
    "ok-deep": "#1e3a28", "ok-text": "#3d8a65", "warn": "#d4a843", "warn-deep": "#8a6e30",
    "err": "#e05a5a", "err-deep": "#3a1818", "err-text": "#c05050",
    "agent-planner": "#7ab4e8", "agent-coder": "#8a82cc", "agent-tester": "#d4a843",
    "agent-reviewer": "#c27ba0", "agent-human": "#4CAF82",
}
for name, value in expected.items():
    assert tokens[f"--noema-{name}"] == value, f"Noema token changed: {name}"
# The phone keeps the app's colour roles (every Color in tokens.dart) and fills each with a Noema value.
dart = (ROOT.parents[2] / "apps/composer_app/lib/theme/tokens.dart").read_text()
app_tokens = dict(re.findall(r"(--app-[\w-]+):\s*([^;]+);", (ROOT / "phone-tokens.css").read_text()))
for name in re.findall(r"const (\w+) = Color\(0xFF[0-9A-Fa-f]{6}\)", dart):
    css = "--app-" + re.sub(r"(?<!^)([A-Z])", r"-\1", name).lower()
    value = app_tokens.get(css, "")
    assert re.fullmatch(r"var\(--noema-[\w-]+\)", value), f"{css} must be a Noema colour, is {value!r}"
    assert value[4:-1] in tokens, f"{css} names an unknown Noema token"
phone_used = set()
for file in [ROOT / "phone.css", *PHONE]:
    text = file.read_text()
    assert "--noema-" not in text, f"Colours reach the phone through its roles (phone-tokens.css): {file.name}"
    phone_used.update(re.findall(r"var\((--app-[\w-]+)\)", text))
assert phone_used <= app_tokens.keys(), f"Undefined phone tokens: {phone_used - app_tokens.keys()}"
used = set()
for file in [ROOT / "base.css", ROOT / "index.html", *DESKTOP]:
    used.update(re.findall(r"var\((--noema-[\w-]+)\)", file.read_text()))
assert used <= tokens.keys(), f"Undefined tokens: {used - tokens.keys()}"
print("ok desktop/phone shells, 30 exact Noema colors, phone colour roles filled from Noema, local fonts, token references")
