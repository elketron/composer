#!/usr/bin/env python3
"""Static checks for docs/mockups/next.

No browser needed: checks the things the render pass cannot know — that each
screen carries the shared shell, uses palette values only, and points at
documents that exist. The visual pass is still a human looking at a render.
"""
from __future__ import annotations

import html.parser
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SCREENS = sorted((ROOT / "screens").glob("*.html"))
INDEX = ROOT / "index.html"


class Scan(html.parser.HTMLParser):
    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.links: list[str] = []
        self.scripts: list[str] = []
        self.style = ""
        self._in_style = False
        self.figures: list[str] = []
        self.title = ""
        self._in_title = False
        self._in_script = False
        self._script = ""

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        a = {k: (v or "") for k, v in attrs}
        if tag == "link" and a.get("rel") == "stylesheet":
            self.links.append(a.get("href", ""))
        if tag == "figure" and "screen" in a.get("class", "").split():
            self.figures.append(a.get("class", ""))
        if tag == "style":
            self._in_style = True
        if tag == "title":
            self._in_title = True
        if tag == "script":
            self._in_script = True
            self._script = ""
        for key in ("href", "src"):
            if a.get(key):
                self.links.append(a[key])

    def handle_endtag(self, tag: str) -> None:
        if tag == "style":
            self._in_style = False
        if tag == "title":
            self._in_title = False
        if tag == "script":
            self._in_script = False
            self.scripts.append(self._script)

    def handle_data(self, data: str) -> None:
        if self._in_style:
            self.style += data
        if self._in_title:
            self.title += data
        if self._in_script:
            self._script += data


def check_screen(path: Path) -> list[str]:
    text = path.read_text()
    scan = Scan()
    scan.feed(text)
    problems: list[str] = []
    rel = path.relative_to(ROOT)

    if "<!doctype html>" not in text.lower():
        problems.append(f"{rel}: missing doctype")
    if not scan.title.strip():
        problems.append(f"{rel}: missing title")
    for needed in ("../tokens.css", "../base.css"):
        if needed not in scan.links:
            problems.append(f"{rel}: missing stylesheet {needed}")
    if len(scan.figures) != 1:
        problems.append(f"{rel}: expected one figure.screen, found {len(scan.figures)}")
    if 'class="fit"' not in text:
        problems.append(f"{rel}: frame is not wrapped in .fit (it would clip below 1360px)")

    # Palette values only inside the screen's own CSS: raw hex would be a new
    # colour sneaking in beside tokens.css.
    for match in re.finditer(r"#[0-9a-fA-F]{3,8}\b", scan.style):
        problems.append(f"{rel}: raw colour {match.group(0)} in <style>; use a token")

    # External resources break the no-build, offline surface.
    for url in scan.links:
        if url.startswith(("http://", "https://", "//")):
            problems.append(f"{rel}: external resource {url}")

    # Local doc links must resolve (they are part of the screen's contract).
    for url in scan.links:
        if url.startswith(("http", "#", "mailto:", "javascript:")) or url.startswith("screens/"):
            continue
        target = (path.parent / url).resolve()
        if target.suffix in ("", ".html", ".md") and not target.exists():
            problems.append(f"{rel}: missing local link {url}")

    return problems


def declarations(path: Path) -> dict[str, str]:
    text = path.read_text()
    return {
        name: value.strip()
        for name, value in re.findall(r"--([a-z0-9-]+)\s*:\s*([^;]+);", text)
    }


def check_tokens() -> list[str]:
    """tokens.css is the shipped palette; keep the colour and radius values equal."""
    shipped = ROOT.parents[2] / "templates/sdk/theme.css"
    local = declarations(ROOT / "tokens.css")
    if not shipped.exists():
        return [f"missing shipped tokens {shipped}"]
    remote = declarations(shipped)
    keys = [
        "bg", "surface", "raised", "overlay", "border", "border-strong", "text", "muted", "faint",
        "accent", "accent-dim", "you", "ok", "warn", "err", "info", "radius-sm", "radius", "gap",
    ]
    problems: list[str] = []
    for key in keys:
        if key not in local or key not in remote:
            problems.append(f"tokens.css: missing --{key} in one side")
        elif local[key].lower() != remote[key].lower():
            problems.append(f"tokens.css: --{key} is {local[key]}, shipped is {remote[key]}")
    return problems


def check_index() -> list[str]:
    text = INDEX.read_text()
    scan = Scan()
    scan.feed(text)
    problems: list[str] = []
    rel = INDEX.relative_to(ROOT)
    if not scan.title.strip():
        problems.append(f"{rel}: missing title")
    for screen in SCREENS:
        if screen.name not in text:
            problems.append(f"{rel}: does not link {screen.name}")
    if "tokens.css" not in scan.links or "base.css" not in scan.links:
        problems.append(f"{rel}: missing tokens.css or base.css")
    for url in scan.links:
        if url.startswith(("http://", "https://", "//")):
            problems.append(f"{rel}: external resource {url}")
        if url.startswith(("http", "#", "mailto:", "javascript:")):
            continue
        target = (INDEX.parent / url).resolve()
        if target.suffix in ("", ".html", ".md") and not target.exists():
            problems.append(f"{rel}: missing local link {url}")
    return problems


def main() -> int:
    problems: list[str] = []
    if not SCREENS:
        problems.append("no screens found")
    for screen in SCREENS:
        problems.extend(check_screen(screen))
    problems.extend(check_tokens())
    if INDEX.exists():
        problems.extend(check_index())
    else:
        problems.append("missing index.html")

    if problems:
        print("mockups/next: FAIL")
        for problem in problems:
            print(f"  - {problem}")
        return 1

    print(f"mockups/next: ok — {len(SCREENS)} screens, palette-only, no external resources")
    return 0


if __name__ == "__main__":
    sys.exit(main())
