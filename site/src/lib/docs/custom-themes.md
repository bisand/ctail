Custom themes are JSON files in the `themes` folder of the [configuration directory](../configuration-files/), which on macOS is:

```
~/Library/Application Support/ctail/themes/
```

Drop a file there and the theme appears in **Settings ▸ Theme** the next time you open Settings. Custom themes require [ctail Pro](../pro/).

## Example

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

## Fields

| Field | Description |
|---|---|
| `name` | Unique identifier, lowercase with hyphens |
| `displayName` | Name shown in the theme picker |
| `dark`, `light` | Colour sets for each mode. Both are required |

## Colour properties

| Property | Used for |
|---|---|
| `bg-primary` | Log view and window background |
| `bg-secondary` | Tab bar, status bar and panels |
| `bg-surface` | Elevated surfaces such as menus |
| `bg-hover` | Hover state |
| `text-primary` | Main text |
| `text-secondary` | Labels and metadata |
| `text-muted` | Line numbers and hints |
| `accent` | Selection, active elements, links |
| `accent-hover` | Accent on hover |
| `border` | Separators |
| `danger` | Errors, invalid regex |
| `success` | Positive status |
| `warning` | Tab warning marker |
| `tab-active` | Active tab background |
| `tab-inactive` | Inactive tab background |
| `badge-color` | Activity badge on inactive tabs |
| `scrollbar-track` | Scrollbar track |
| `scrollbar-thumb` | Scrollbar thumb |

Values are hex colours. A missing property falls back to the previously active theme's value, so include all eighteen in both modes for predictable results.

## Tips

- **Override a built-in theme** by using its `name`, for instance `"name": "nord"`. Your file takes priority.
- **Adapt a VS Code theme** by mapping `editor.background` to `bg-primary`, `sideBar.background` to `bg-secondary`, `editor.foreground` to `text-primary`, `focusBorder` to `accent`, `editorError.foreground` to `danger`, and the ANSI green and yellow to `success` and `warning`.
- Keep at least a 4.5:1 contrast ratio between `text-primary` and `bg-primary`.
- Validate the file with `python3 -m json.tool mytheme.json` if it does not show up.
- Share a theme by sending the JSON file. The cross-platform edition of ctail uses the same format.
