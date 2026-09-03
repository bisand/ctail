#!/usr/bin/env python3
"""Generate core/src/themes_generated.rs from legacy/wails/internal/config/themes.go.

The Go file defines 21 themes as funcs returning Theme{ Name, DisplayName,
Dark: ThemeColors{...}, Light: ThemeColors{...} }. We parse those literal
blocks and emit a Rust `built_in_themes()` for the engine crate; the Swift app
gets them through the FFI.
"""
import re, sys, pathlib

GO = pathlib.Path(sys.argv[1])
OUT = pathlib.Path(sys.argv[2])

FIELD_MAP = {
    "BgPrimary": "bg_primary", "BgSecondary": "bg_secondary", "BgSurface": "bg_surface",
    "BgHover": "bg_hover", "TextPrimary": "text_primary", "TextSecondary": "text_secondary",
    "TextMuted": "text_muted", "Accent": "accent", "AccentHover": "accent_hover",
    "Border": "border", "Danger": "danger", "Success": "success", "Warning": "warning",
    "TabActive": "tab_active", "TabInactive": "tab_inactive", "BadgeColor": "badge_color",
    "ScrollTrack": "scrollbar_track", "ScrollThumb": "scrollbar_thumb",
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

def colors_literal(d, indent):
    pad = " " * indent
    parts = [f'{pad}    {k}: "{d.get(k, "#000000")}".into(),' for k in ORDER]
    return f"ThemeColors {{\n" + "\n".join(parts) + f"\n{pad}}}"

lines = [
    "// AUTO-GENERATED from legacy/wails/internal/config/themes.go by macos/scripts/gen_themes.py.",
    "// Do not edit by hand; rerun `make -C macos themes` after changing the Go themes.",
    "",
    "use crate::models::{Theme, ThemeColors};",
    "",
    "/// The built-in themes, in catalogue order.",
    "pub fn built_in_themes() -> Vec<Theme> {",
    "    vec![",
]
for t in themes:
    lines.append("        Theme {")
    lines.append(f'            name: "{t["name"]}".into(),')
    lines.append(f'            display_name: "{t["displayName"]}".into(),')
    lines.append(f'            dark: {colors_literal(t["dark"], 12)},')
    lines.append(f'            light: {colors_literal(t["light"], 12)},')
    lines.append("            built_in: true,")
    lines.append("        },")
lines += ["    ]", "}", ""]

OUT.write_text("\n".join(lines))
print(f"wrote {OUT} with {len(themes)} themes")
