## Follow mode

With **Follow** ticked in the status bar, ctail behaves like `tail -f`: new lines are appended as they are written and the view stays pinned to the end of the file.

- Scroll up and following pauses automatically, so you can read history without the view jumping away.
- Scroll back to the bottom and following resumes.
- You can also toggle the checkbox by hand.

## How scrollback works

ctail is built so that file size never matters:

- **Tail first.** When a file opens, ctail reads only its last part and shows it immediately. A background task then walks the file to build an index of line offsets. The status bar shows *counting lines…* until it finishes.
- **Virtualized view.** The log surface is a native table view that only creates the rows currently on screen. Ten lines or ten million, the amount of work per frame is the same.
- **Windowed reads.** As you scroll, ctail reads the ranges it needs directly from disk using the index, and lets ranges you have scrolled past go again. Memory use stays flat.
- **Memory indicator.** The status bar shows the app's real memory footprint, the same figure Activity Monitor reports, so you can see it for yourself.

The **Buffer size** and **Scrollback** settings tune how many lines are kept in memory. The defaults suit almost everyone; see [Settings](../settings/).

## Selecting and copying

- Click a line to select it. <kbd>⇧</kbd>-click extends the selection, <kbd>⌘</kbd>-click adds individual lines, and click-drag selects a range.
- <kbd>⌘A</kbd> selects every loaded line, <kbd>⌘C</kbd> copies the selection as plain text.
- Right-click in the log for the same actions plus **Ask AI about logs**.

## Long lines

Long lines extend past the window edge and scroll horizontally. Enable **View ▸ Word Wrap** (<kbd>⌘⌥W</kbd>) to wrap them instead. **View ▸ Show Line Numbers** (<kbd>⌘⇧L</kbd>) toggles the gutter. Both take effect immediately and are remembered.
