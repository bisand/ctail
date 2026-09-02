<script>
	import Shot from '$lib/components/Shot.svelte';
</script>

ctail (short for *colour tail*) is a native macOS log viewer. It shows the end of a file, streams new lines as they are written, and colours them with regular-expression rules so the important entries stand out.

<Shot src="main.webp" alt="ctail tailing a web server log" />

## Install

Install ctail from the [Mac App Store](../../download/). It runs on macOS 13 Ventura or later on both Apple silicon and Intel Macs. Updates arrive through the App Store.

If you prefer to build from source, the app is a Swift package in the `macos/` folder of the [GitHub repository](https://github.com/bisand/ctail). See the [download page](../../download/) for the commands.

## First launch

On first launch ctail creates its configuration folder with a default **Common Logs** highlighting profile and shows the file picker. Pick any text or log file and it opens in a tab, already following.

The window has four parts:

- **Tab bar** across the top, one tab per open file.
- **Log view** in the middle. Line numbers are on by default and can be toggled from the View menu.
- **Search bar**, hidden until you press <kbd>⌘F</kbd>.
- **Status bar** at the bottom with the file name, line count, memory footprint, and the **Follow** checkbox.

## The basics in one minute

1. <kbd>⌘O</kbd> opens a file. You can select several at once.
2. Scroll up to look at history. Following pauses on its own; scroll to the bottom and it resumes.
3. <kbd>⌘F</kbd> searches. The filter toggle in the search bar hides every line that does not match.
4. **View ▸ Profiles & Rules…** opens the rule editor where you change colours and patterns.
5. **View ▸ Toggle Theme** switches between the dark and light variant of the current theme.

## Free and Pro

ctail is free to use. Two files can be open at once and the Catppuccin theme is included. **ctail Pro**, a one-time in-app purchase, removes the tab limit, unlocks all 21 themes and custom themes, and enables the AI assistant. See [ctail Pro](../pro/).

## Where next

- [Opening files](../opening-files/) covers Finder integration, network shares and recent files.
- [Highlighting rules](../highlighting-rules/) explains how colouring works.
- [Keyboard shortcuts](../keyboard-shortcuts/) lists every shortcut.
