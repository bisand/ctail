There are several ways to get a file into ctail. All of them open a new tab that starts following immediately.

## From inside the app

- Press <kbd>⌘O</kbd> or choose **File ▸ Open…**. The native file dialog allows multiple selection, so you can open a whole set of logs at once.
- **File ▸ Open Recent** lists the last files you opened. **Clear Recent** empties the list.

## From Finder

ctail registers itself as a viewer for `.log`, `.txt` and `.csv` files:

- Right-click a file in Finder and choose **Open With ▸ ctail**.
- Drag a file onto the ctail icon in the Dock.
- Set ctail as the default app for `.log` files in the Finder Info panel and double-click from then on.

Because the App Store build runs in the macOS sandbox, ctail saves a security-scoped bookmark for every file you open. That is what lets it reopen the same files after a relaunch without asking again.

## Files on network shares

Files on SMB, NFS or SSHFS mounts work. ctail polls the file on a timer instead of relying on filesystem events, which network filesystems frequently fail to deliver.

- If the share stalls, the tab shows a warning marker and the status bar reports the problem. The rest of the app stays responsive.
- The marker clears automatically once the file is reachable again.
- The **Poll interval** and **Read timeout** settings control how often ctail checks and how long it waits. See [Settings](../settings/).

## Very large files

There is no size limit. ctail reads the tail of the file first, so even a multi-gigabyte log is on screen within milliseconds. Line offsets are indexed in the background; while that runs the status bar says *counting lines…* and then shows the total. See [Following & scrolling](../following-and-scrolling/) for how scrollback works.

## Rotation and truncation

Log rotation is detected by inode. When `logrotate` or your application replaces the file at the same path, ctail notices and switches to the new file. If a file is truncated to zero and rewritten, the view resets rather than showing stale content.

## Free tier limit

Without ctail Pro, two files can be open at once. Opening a third shows the Pro window. Close a tab or [unlock Pro](../pro/) to continue.
