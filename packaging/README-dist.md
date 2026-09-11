# ctail

A log viewer that follows files as they grow: the tail of a large file opens at
once, lines are coloured by highlighting rules, search covers the whole file
rather than what is on screen, and the tabs come back the way they were left.

This is the Linux and Windows build. On a Mac, get ctail from the Mac App Store:
https://apps.apple.com/app/id6808019248

## Running it

    ctail [FILE...]     open each file in a tab; with none, the tabs from last time
    ctail --version     print the version and exit

Settings, highlighting profiles and themes are kept in `~/.config/ctail` on
Linux and `%APPDATA%\ctail` on Windows. Set `CTAIL_CONFIG_DIR` to use another
directory.

The window is drawn through the GPU where one can present to it, and in software
where none can, so a GPU is used but never required. `CTAIL_PRESENT=software`
chooses software drawing outright.

## More

Source, issues and the user manual: https://github.com/bisand/ctail

ctail is released under the MIT licence; see `LICENSE`.
