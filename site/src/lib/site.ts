export const site = {
	name: 'ctail',
	tagline: 'The log viewer that keeps up.',
	description:
		'ctail is a fast native macOS log viewer: tail -f with regex color highlighting, tabs, search, 21 themes and an AI assistant. Built for huge files.',
	github: 'https://github.com/bisand/ctail',
	issues: 'https://github.com/bisand/ctail/issues',
	discussions: 'https://github.com/bisand/ctail/discussions',
	releases: 'https://github.com/bisand/ctail/releases',
	latestRelease: 'https://github.com/bisand/ctail/releases/latest',
	security: 'https://github.com/bisand/ctail/blob/main/SECURITY.md',
	license: 'https://github.com/bisand/ctail/blob/main/LICENSE',
	appStore: 'https://apps.apple.com/app/ctail/id0000000000', // TODO: replace with the real App Store link once live
	author: 'André Biseth',
	authorUrl: 'https://github.com/bisand',
	version: '0.9.9',
	minMacOS: 'macOS 13 Ventura'
};

export const nav = [
	{ href: '/features/', label: 'Features' },
	{ href: '/download/', label: 'Download' },
	{ href: '/docs/', label: 'Docs' },
	{ href: '/support/', label: 'Support' }
];

export type DocPage = { slug: string; title: string; group: string };

export const docPages: DocPage[] = [
	{ slug: 'getting-started', title: 'Getting started', group: 'Basics' },
	{ slug: 'opening-files', title: 'Opening files', group: 'Basics' },
	{ slug: 'tabs', title: 'Tabs', group: 'Basics' },
	{ slug: 'following-and-scrolling', title: 'Following & scrolling', group: 'Basics' },
	{ slug: 'search', title: 'Search & filter', group: 'Basics' },
	{ slug: 'highlighting-rules', title: 'Highlighting rules', group: 'Highlighting' },
	{ slug: 'profiles', title: 'Rule profiles', group: 'Highlighting' },
	{ slug: 'themes', title: 'Themes', group: 'Highlighting' },
	{ slug: 'custom-themes', title: 'Custom themes', group: 'Highlighting' },
	{ slug: 'ai-assistant', title: 'AI assistant', group: 'AI' },
	{ slug: 'ai-providers', title: 'AI providers', group: 'AI' },
	{ slug: 'settings', title: 'Settings', group: 'Reference' },
	{ slug: 'menus', title: 'Menus & context menus', group: 'Reference' },
	{ slug: 'keyboard-shortcuts', title: 'Keyboard shortcuts', group: 'Reference' },
	{ slug: 'pro', title: 'ctail Pro', group: 'Reference' },
	{ slug: 'configuration-files', title: 'Configuration files', group: 'Reference' },
	{ slug: 'troubleshooting', title: 'Troubleshooting', group: 'Reference' }
];
