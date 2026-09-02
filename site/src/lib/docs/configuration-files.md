ctail keeps everything in one folder:

```
~/Library/Application Support/ctail/
├── settings.json        # settings, window state, open tabs, recent files, AI config
├── profiles/
│   └── common-logs.json # one JSON file per highlighting profile
├── themes/
│   └── my-theme.json    # custom themes (optional)
└── bookmarks/           # security-scoped bookmarks for opened files (sandbox)
```

Set the `CTAIL_CONFIG_DIR` environment variable to use a different folder, which is useful for testing or for keeping separate setups.

## settings.json

All values from the [Settings](../settings/) window plus session state: window frame, the list of open tabs with their labels and colours, the active tab, the active profile, recent files, and AI provider details. It is rewritten whenever something changes, so tabs survive a crash.

The AI API key is stored here in plain text. Keep the folder private if that matters to you, or use a local model.

## profiles/*.json

One file per profile. The format is described under [Rule profiles](../profiles/). Files are safe to copy between machines and are compatible with the cross-platform edition of ctail.

## themes/*.json

Custom themes. See [Custom themes](../custom-themes/).

## Resetting

Quit ctail and delete the folder to start from scratch. The default **Common Logs** profile is recreated on the next launch.
