#!/usr/bin/env python3
"""Generate Themes.generated.swift from internal/config/themes.go.

The Go file defines 21 themes as funcs returning Theme{ Name, DisplayName,
Dark: ThemeColors{...}, Light: ThemeColors{...} }. We parse those literal
blocks and emit a Swift array of Theme values.
"""
import re, sys, pathlib

GO = pathlib.Path(sys.argv[1])
OUT = pathlib.Path(sys.argv[2])

FIELD_MAP = {
    "BgPrimary": "bgPrimary", "BgSecondary": "bgSecondary", "BgSurface": "bgSurface",
    "BgHover": "bgHover", "TextPrimary": "textPrimary", "TextSecondary": "textSecondary",
    "TextMuted": "textMuted", "Accent": "accent", "AccentHover": "accentHover",
    "Border": "border", "Danger": "danger", "Success": "success", "Warning": "warning",
    "TabActive": "tabActive", "TabInactive": "tabInactive", "BadgeColor": "badgeColor",
    "ScrollTrack": "scrollbarTrack", "ScrollThumb": "scrollbarThumb",
}
ORDER = list(FIELD_MAP.values())

text = GO.read_text()
themes = []
cur = None
section = None  # 'dark' | 'light'

kv = re.compile(r'^\s*([A-Za-z]+):\s*"([^"]*)"')

for line in text.splitlines():
    m = re.search(r'Name:\s*"([^"]+)"', line)
    if m and "DisplayName" not in line:
        cur = {"name": m.group(1), "displayName": "", "dark": {}, "light": {}}
        themes.append(cur)
        section = None
        continue
    m = re.search(r'DisplayName:\s*"([^"]+)"', line)
    if m and cur is not None:
        cur["displayName"] = m.group(1)
        continue
    if "Dark: ThemeColors" in line:
        section = "dark"; continue
    if "Light: ThemeColors" in line:
        section = "light"; continue
    if cur is None or section is None:
        continue
    m = kv.match(line)
    if m and m.group(1) in FIELD_MAP:
        cur[section][FIELD_MAP[m.group(1)]] = m.group(2)

def colors_literal(d):
    parts = [f'{k}: "{d.get(k, "#000000")}"' for k in ORDER]
    return "ThemeColors(" + ", ".join(parts) + ")"

lines = [
    "// AUTO-GENERATED from internal/config/themes.go by scripts/gen_themes.py.",
    "// Do not edit by hand; rerun `make themes` after changing the Go themes.",
    "import Foundation",
    "",
    "extension ThemeCatalog {",
    "    static let builtIns: [Theme] = [",
]
for t in themes:
    lines.append(f'        Theme(name: "{t["name"]}", displayName: "{t["displayName"]}",')
    lines.append(f'              dark: {colors_literal(t["dark"])},')
    lines.append(f'              light: {colors_literal(t["light"])}),')
lines += ["    ]", "}", ""]

OUT.write_text("\n".join(lines))
print(f"wrote {OUT} with {len(themes)} themes")
