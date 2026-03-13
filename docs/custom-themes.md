# Creating Custom Themes for ctail

ctail supports custom color themes via JSON files. You can create your own themes to match your preferred color palette, or tweak an existing built-in theme.

> 🎨 ctail's built-in theme palettes are inspired by and adapted from [OpenCode](https://github.com/anomalyco/opencode) by [Anomaly](https://anomaly.co/). If you enjoy the themes, check out their project — it's a fantastic terminal-based AI coding tool with beautiful design.

## Quick Start

1. Create a JSON file in your themes directory:

   | Platform | Path |
   |----------|------|
   | Linux | `~/.config/ctail/themes/` |
   | Windows | `%APPDATA%\ctail\themes\` |
   | macOS | `~/Library/Application Support/ctail/themes/` |

2. Add the following JSON structure (example: a custom "Ocean" theme):

   ```json
   {
     "name": "ocean",
     "displayName": "Ocean",
     "dark": {
       "bg-primary": "#1b2838",
       "bg-secondary": "#151e2b",
       "bg-surface": "#243447",
       "bg-hover": "#2d4057",
       "text-primary": "#c4d6e8",
       "text-secondary": "#a8bdd0",
       "text-muted": "#5c7a94",
       "accent": "#5fb3b3",
       "accent-hover": "#6bc5c5",
       "border": "#2d4057",
       "danger": "#ec5f67",
       "success": "#99c794",
       "warning": "#fac863",
       "tab-active": "#1b2838",
       "tab-inactive": "#151e2b",
       "badge-color": "#fac863",
       "scrollbar-track": "#151e2b",
       "scrollbar-thumb": "#2d4057"
     },
     "light": {
       "bg-primary": "#f4f7fa",
       "bg-secondary": "#e8ecf0",
       "bg-surface": "#d8dee6",
       "bg-hover": "#c8d0da",
       "text-primary": "#2b3e50",
       "text-secondary": "#3e5468",
       "text-muted": "#8899aa",
       "accent": "#3d8a8a",
       "accent-hover": "#4a9e9e",
       "border": "#c8d0da",
       "danger": "#c0392b",
       "success": "#27ae60",
       "warning": "#d4a017",
       "tab-active": "#f4f7fa",
       "tab-inactive": "#e8ecf0",
       "badge-color": "#d4a017",
       "scrollbar-track": "#e8ecf0",
       "scrollbar-thumb": "#c8d0da"
     }
   }
   ```

3. Restart ctail (or switch away and back to the theme) — your theme appears in the **Settings → Theme** dropdown.

## Theme JSON Reference

### Required Fields

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Unique identifier (lowercase, hyphens). Used internally. |
| `displayName` | string | Human-readable name shown in the theme picker. |
| `dark` | object | Color definitions for dark mode. |
| `light` | object | Color definitions for light mode. |

### Color Properties

Both `dark` and `light` objects use the same set of CSS custom property names:

| Property | Used For |
|----------|----------|
| `bg-primary` | Main background color (log view, window) |
| `bg-secondary` | Secondary background (sidebar, panels) |
| `bg-surface` | Elevated surfaces (dropdowns, cards) |
| `bg-hover` | Hover state background |
| `text-primary` | Main text color |
| `text-secondary` | Secondary text (labels, metadata) |
| `text-muted` | Muted text (timestamps, line numbers) |
| `accent` | Primary accent color (links, active elements) |
| `accent-hover` | Accent color on hover |
| `border` | Border color for separators and outlines |
| `danger` | Error/danger color (e.g., FATAL, ERROR highlights) |
| `success` | Success color (e.g., status indicators) |
| `warning` | Warning color (e.g., tab warning indicator, badges) |
| `tab-active` | Active tab background |
| `tab-inactive` | Inactive tab background |
| `badge-color` | Update badge dot color on inactive tabs |
| `scrollbar-track` | Scrollbar track background |
| `scrollbar-thumb` | Scrollbar thumb color |

All values must be valid CSS color strings (hex `#rrggbb` recommended).

## Tips for Creating Themes

### Start from a Built-In Theme

The easiest way to create a custom theme is to export an existing one as a starting point:

1. Open the themes directory (see paths above).
2. Create a new `.json` file.
3. Copy the structure from the example above.
4. Adjust colors to your liking.

### Color Relationships

For a cohesive theme, follow these guidelines:

- **`bg-secondary`** should be slightly darker (dark mode) or lighter (light mode) than `bg-primary`
- **`bg-surface`** should be a step between `bg-primary` and `bg-hover`
- **`text-secondary`** should be between `text-primary` and `text-muted` in brightness
- **`tab-active`** typically matches `bg-primary`; **`tab-inactive`** matches `bg-secondary`
- **`scrollbar-track`** typically matches `bg-secondary`; **`scrollbar-thumb`** matches `bg-hover` or `border`

### Dark Mode Considerations

- Keep sufficient contrast between `text-primary` and `bg-primary` (aim for WCAG AA: 4.5:1 ratio)
- `danger`, `success`, and `warning` colors should be visible against `bg-primary`
- Avoid pure white (`#ffffff`) text on very dark backgrounds — slightly desaturated text is easier on the eyes

### Light Mode Considerations

- Use darker, more saturated accent colors than in dark mode
- `bg-primary` should be off-white rather than pure white for reduced eye strain
- Ensure `text-muted` is still readable against `bg-primary`

## Overriding Built-In Themes

If you create a custom theme with the same `name` as a built-in theme (e.g., `"name": "catppuccin"`), your custom version takes priority. This lets you tweak built-in themes without modifying the application.

## Example: Adapting a VS Code Theme

Many VS Code color themes publish their palettes. To adapt one for ctail:

1. Find the theme's color definitions (usually in a `package.json` or theme JSON file)
2. Map the VS Code token colors to ctail properties:

   | VS Code Token | ctail Property |
   |---------------|----------------|
   | `editor.background` | `bg-primary` |
   | `sideBar.background` | `bg-secondary` |
   | `editor.foreground` | `text-primary` |
   | `focusBorder` / `button.background` | `accent` |
   | `editorError.foreground` | `danger` |
   | `terminal.ansiGreen` | `success` |
   | `terminal.ansiYellow` | `warning` |
   | `tab.activeBackground` | `tab-active` |
   | `tab.inactiveBackground` | `tab-inactive` |
   | `editorWidget.border` | `border` |

3. For colors not directly available, derive them from the base palette (darken/lighten backgrounds, desaturate text).

## Sharing Themes

Custom theme files are standalone JSON — share them by copying the `.json` file. Recipients just need to place it in their themes directory and restart ctail.

## Troubleshooting

- **Theme doesn't appear in dropdown**: Ensure the file has a `.json` extension and is valid JSON. Check for syntax errors with `python3 -m json.tool mytheme.json`.
- **Colors look wrong**: Verify all 18 color properties are present in both `dark` and `light` objects. Missing properties fall back to the previously active theme's values.
- **Theme name conflicts**: If two custom theme files have the same `name`, the last one loaded wins (alphabetical by filename). Use unique names.
