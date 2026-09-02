Highlighting rules are what turn a wall of grey text into something you can read at a glance. Each rule is a regular expression paired with a style. Rules are grouped into [profiles](../profiles/); the active profile applies to every open tab.

![The Profiles and Rules window](../../screenshots/profiles.webp)

## Anatomy of a rule

| Property | Description |
|---|---|
| **Name** | A label shown in the rule list |
| **Pattern** | A regular expression, for example `\bERROR\b` |
| **Match type** | **Entire line** colours the whole line; **Match only** colours just the text the pattern matched |
| **Foreground** | Text colour |
| **Background** | Optional background colour |
| **Bold / Italic** | Text style |
| **Enabled** | Untick to keep a rule without applying it |

Patterns use the ICU regular expression syntax that macOS provides, which covers everything you would expect from PCRE: character classes, alternation, groups, anchors, `\b` word boundaries and inline flags such as `(?i)` for case-insensitive matching.

## Priority

Rules are applied in list order and **later rules win**. If two rules match the same text, the one lower in the list decides the style. A good layout is:

1. Broad, low-priority rules at the top, such as colouring timestamps.
2. Level rules like INFO and DEBUG in the middle.
3. Loud rules like ERROR and FATAL at the bottom, so nothing overrides them.

A *Match only* rule inside a line already coloured by an *Entire line* rule keeps its own colour only if it sits lower in the list than the line rule.

## Editing rules

1. Open **View ▸ Profiles & Rules…**.
2. Pick the profile to edit from the list on the left.
3. Select a rule to edit it, or click **+** to add one.
4. Type the pattern. The editor validates it as you type and shows an error for an invalid expression.
5. Set colours and style. The **Preview** line renders the rule against the current theme so you can judge contrast.
6. Reorder with the ▲ and ▼ buttons.
7. Click **Save Profile**. The change applies to every open tab immediately.

## The default profile

New installs ship with **Common Logs**:

| Rule | Pattern | Type | Style |
|---|---|---|---|
| Fatal | `\bFATAL\b` | Entire line | White on red, bold |
| Error | `\bERROR\b` | Entire line | Red on dark red, bold |
| Warning | `\bWARN(ING)?\b` | Entire line | Yellow on dark yellow |
| Info | `\bINFO?\b` | Match only | Blue |
| Debug | `\bDEBUG\b` | Match only | Grey |
| Timestamp | `\d{4}-\d{2}-\d{2}T?\d{2}:\d{2}:\d{2}` | Match only | Green |

## Let the AI write the rules

If you have [ctail Pro](../pro/) and a configured provider, **Generate Rules Profile** in the [AI assistant](../ai-assistant/) reads the current log and produces a complete profile with patterns, colours and ordering. The result is an ordinary profile that you can edit afterwards.
