Press <kbd>⌘F</kbd> or choose **Edit ▸ Find…** to open the inline search bar above the log. It works like the find widget in VS Code.

![The search bar with matches for ERROR](../../screenshots/search.webp)

## Toggles

| Toggle | Meaning |
|---|---|
| **Aa** | Case sensitive |
| **ab** | Whole word only |
| **.\*** | Treat the query as a regular expression |
| **Filter** | Show only matching lines |

## Navigating matches

The counter shows the current match and the total, for example *3/42*. Move between matches with:

- <kbd>Enter</kbd> or the ↓ button for the next match
- <kbd>⇧Enter</kbd> or the ↑ button for the previous match

Navigation wraps around at both ends of the file.

## Filter mode

Turn on **Filter** and every line that does not match disappears, leaving a focused view of just the hits. Turn it off to see the full log again with the matches still highlighted. Filter mode combined with a regex like `ERROR|WARN` is a quick way to triage a busy log.

## Behaviour notes

- Search highlighting is drawn on top of your colour rules, so you keep the context the rules provide.
- The search re-runs automatically when you switch tabs or when new lines arrive.
- An invalid regular expression shows *bad regex* in the counter instead of failing silently.
- <kbd>Esc</kbd> or the × button closes the bar and clears the highlighting.
