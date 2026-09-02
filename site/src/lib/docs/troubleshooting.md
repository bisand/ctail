## A file on a network share is slow or shows a warning

ctail polls files instead of using filesystem events, which is what makes network mounts work at all. If the share is slow:

- Raise **Poll interval** in Settings so ctail asks less often.
- Raise **Read timeout** if reads legitimately take longer than the default.
- The warning marker on the tab clears on its own when the share responds again.

## Highlighting does not apply

- Check the pattern in **Profiles & Rules**. An invalid expression is flagged in the editor.
- Make sure the rule is **enabled**.
- Remember that later rules win. A broad rule lower in the list can override a specific one above it.
- Confirm the profile you edited is the active one.

## The third file will not open

The free tier allows two open files. Close a tab or unlock [ctail Pro](../pro/).

## A theme I picked reverted to Catppuccin

Themes other than Catppuccin are part of Pro. If Pro is not unlocked ctail falls back to the free theme.

## Tabs were not restored

- Check that **Restore tabs on launch** is enabled in Settings.
- Under the App Store sandbox, ctail needs the security-scoped bookmark it saved when you opened the file. If the file moved, open it again.

## The AI assistant does nothing

- A provider must be configured under Settings ▸ AI. See [AI providers](../ai-providers/).
- For Copilot, complete the browser sign-in and make sure your subscription is active.
- For a local server, make sure it is running and the endpoint is right.

## Update check fails

The check talks to GitHub's releases API. It is harmless when it fails and can be disabled in Settings. App Store installs are updated by the App Store anyway.

## Still stuck?

Open an issue on [GitHub](https://github.com/bisand/ctail/issues) with your macOS version, the ctail version from **About ctail**, and what you were doing. See the [support page](../../support/) for tips on a good report.
