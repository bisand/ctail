A profile is a named set of [highlighting rules](../highlighting-rules/). Keep one per log format: one for nginx access logs, one for your application, one for syslog.

## Managing profiles

Open **View ▸ Profiles & Rules…**. The list on the left shows every profile.

- **Add** a profile with the **+** button and give it a name.
- **Delete** a profile with **−**. At least one profile must remain.
- **Activate** a profile by selecting it and saving. The active profile applies to all open tabs and is remembered between launches.

## Where profiles live

Each profile is a JSON file in the `profiles` folder of the [configuration directory](../configuration-files/). The file name is derived from the profile name. Because they are plain JSON you can:

- Copy a profile to another Mac.
- Share it with a colleague.
- Keep it in version control alongside the project that produces the log.
- Reuse it with the cross-platform edition of ctail, which understands the same format.

## Profile format

```json
{
  "name": "Common Logs",
  "rules": [
    {
      "name": "Error",
      "pattern": "\\bERROR\\b",
      "matchType": "line",
      "foreground": "#ff6b6b",
      "background": "#3a1b1b",
      "bold": true,
      "italic": false,
      "enabled": true
    }
  ]
}
```

`matchType` is `line` or `match`. Colours are hex strings. Rule order in the array is the priority order, with later rules winning.

## AI-generated profiles

With [ctail Pro](../pro/), the [AI assistant](../ai-assistant/) can create a profile from the log you have open. It is saved as a normal profile named after the number of rules it produced, and becomes the active profile so you see the result at once. Rename or edit it like any other.
