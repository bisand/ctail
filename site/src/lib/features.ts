export type Feature = {
	id: string;
	title: string;
	blurb: string;
	icon: string;
	details: string[];
	shot?: string;
	shotAlt?: string;
	doc: string;
};

export const features: Feature[] = [
	{
		id: 'tailing',
		title: 'Real-time tailing',
		blurb: 'Follow mode streams new lines the instant they land, like tail -f with a proper UI.',
		icon: 'play',
		details: [
			'Follow mode appends new lines and keeps the view pinned to the end of the file.',
			'Scroll up to inspect history and following pauses automatically. Scroll back to the bottom and it resumes.',
			'Log rotation is detected by inode, so when logrotate swaps in a fresh file ctail switches with it.',
			'Truncation is handled too: if a file is emptied and rewritten, the view resets instead of showing stale data.',
			'Partial lines are buffered until the newline arrives, so you never see half-written entries.'
		],
		shot: 'main-dark.webp',
		shotAlt: 'ctail main window tailing a web server log in the dark Catppuccin theme',
		doc: 'following-and-scrolling'
	},
	{
		id: 'huge-files',
		title: 'Built for huge files',
		blurb: 'Open a multi-gigabyte log and start reading the tail immediately. Memory stays flat.',
		icon: 'bolt',
		details: [
			'Tail-first reads: ctail shows the end of the file at once and indexes line offsets in the background.',
			'The log surface is a virtualized native table view. Only the rows on screen are rendered, whatever the file size.',
			'Scrollback is windowed. Earlier ranges are read from disk on demand as you scroll, then released again.',
			'A live memory indicator in the status bar shows the app footprint, so you can see it stay flat.',
			'Reads run off the main thread with a configurable timeout, so a slow disk never freezes the window.'
		],
		doc: 'following-and-scrolling'
	},
	{
		id: 'highlighting',
		title: 'Regex highlighting',
		blurb: 'Colour whole lines or just the matched text with your own rules. Errors jump out, noise fades.',
		icon: 'palette',
		details: [
			'Each rule is a regular expression with a foreground colour, optional background, bold and italic.',
			'Choose between colouring the entire line or only the matched span.',
			'Rules are ordered. Later rules win, so you can layer a broad rule under a specific one.',
			'The rule editor shows a live preview of every rule against the current theme.',
			'The built-in Common Logs profile covers FATAL, ERROR, WARN, INFO, DEBUG and ISO timestamps out of the box.'
		],
		shot: 'profiles.webp',
		shotAlt: 'The Profiles and Rules editor window with the Common Logs profile',
		doc: 'highlighting-rules'
	},
	{
		id: 'profiles',
		title: 'Rule profiles',
		blurb: 'Keep one set of rules for nginx, another for your app, another for syslog. Switch in one click.',
		icon: 'layers',
		details: [
			'A profile is a named collection of rules stored as plain JSON in your config folder.',
			'Create, rename and delete profiles from the Profiles & Rules window.',
			'The active profile applies to every open tab and is remembered between launches.',
			'Let the AI assistant generate a profile for an unfamiliar log format in seconds.'
		],
		doc: 'profiles'
	},
	{
		id: 'search',
		title: 'Search and filter',
		blurb: 'A VS Code-style find bar with case, whole-word and regex toggles, plus a filter mode that hides everything else.',
		icon: 'search',
		details: [
			'Press ⌘F to open the inline search bar. Matches are highlighted and counted.',
			'Step through matches with Enter and Shift-Enter, or the arrow buttons.',
			'Toggle case sensitivity, whole-word matching and regular expressions.',
			'Filter mode shows only matching lines, which turns a noisy log into a focused view.',
			'Search highlighting layers on top of your colour rules, so context is never lost.'
		],
		shot: 'search.webp',
		shotAlt: 'The inline search bar with ERROR matches highlighted and counted',
		doc: 'search'
	},
	{
		id: 'tabs',
		title: 'Tabs',
		blurb: 'Many files, one window. Reorder by drag, rename, colour-code and jump between them from the keyboard.',
		icon: 'tabs',
		details: [
			'Every file gets a tab. Drag to reorder, double-click to rename, right-click for colours and file actions.',
			'Ctrl-Tab and Ctrl-Shift-Tab cycle tabs. ⌘W closes, ⌘⇧T reopens the last closed tab.',
			'Inactive tabs show a badge when new lines arrive, so you notice activity without switching.',
			'A warning marker appears when a file becomes unreachable and clears when it comes back.',
			'Open tabs are restored on the next launch, even after a crash.'
		],
		doc: 'tabs'
	},
	{
		id: 'themes',
		title: '21 themes, dark and light',
		blurb: 'Catppuccin, Nord, Tokyo Night, Gruvbox, Dracula, Solarized and more. Or bring your own JSON theme.',
		icon: 'moon',
		details: [
			'Every theme ships with a dark and a light variant. Toggle from the View menu.',
			'Custom themes are a single JSON file dropped into the themes folder.',
			'Override a built-in theme by reusing its name, or adapt a VS Code palette in minutes.',
			'The rule editor previews rules against the active theme so your colours always read well.'
		],
		shot: 'main-light.webp',
		shotAlt: 'ctail in the light Catppuccin variant',
		doc: 'themes'
	},
	{
		id: 'ai',
		title: 'AI assistant',
		blurb: 'Ask what went wrong, get a plain-English answer, or generate a full highlighting profile from the log in front of you.',
		icon: 'sparkles',
		details: [
			'Ask questions about the current log. The assistant sees the log text and answers in the dialog.',
			'Generate Rules Profile analyses the open file and writes a complete rule set with sensible colours.',
			'Works with OpenAI, Anthropic, GitHub Models, GitHub Copilot, or any OpenAI-compatible server such as Ollama or LM Studio.',
			'Nothing is sent anywhere until you press Ask. Keys stay on your Mac.'
		],
		shot: 'ai.webp',
		shotAlt: 'The AI assistant window explaining a web server log',
		doc: 'ai-assistant'
	},
	{
		id: 'network',
		title: 'Safe on network mounts',
		blurb: 'NFS, SMB and SSHFS shares are polled, not watched, so a flaky mount never hangs the app.',
		icon: 'globe',
		details: [
			'ctail polls files on a configurable interval instead of relying on filesystem events that network mounts often drop.',
			'Every read has a timeout. A stalled share shows a warning on the tab and the rest of the app keeps working.',
			'The poll interval and read timeout are adjustable in Settings.'
		],
		doc: 'settings'
	},
	{
		id: 'native',
		title: 'Truly native macOS',
		blurb: 'Swift and AppKit, no web view, no Electron. Launches in a blink and feels like it belongs.',
		icon: 'apple',
		details: [
			'A real menu bar, native context menus, native file dialogs and Finder integration.',
			'Registered as a viewer for .log, .txt and .csv, so Open With and double-click work.',
			'Sandboxed for the Mac App Store, with security-scoped bookmarks so your files reopen after a relaunch.',
			'Window position, open tabs, active profile and every setting survive restarts.',
			'Built-in update check against GitHub releases.'
		],
		shot: 'settings.webp',
		shotAlt: 'The native Settings window with Appearance, Behavior, Updates and AI Assistant tabs',
		doc: 'getting-started'
	}
];
