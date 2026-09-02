<script lang="ts">
	import { base } from '$app/paths';
	import { site } from '$lib/site';
	import { features } from '$lib/features';
	import Icon from '$lib/components/Icon.svelte';
	import Shot from '$lib/components/Shot.svelte';
	import Seo from '$lib/components/Seo.svelte';

	const themes = [
		['nord', 'Nord'],
		['tokyo-night', 'Tokyo Night'],
		['gruvbox', 'Gruvbox'],
		['dracula', 'Dracula'],
		['one-dark', 'One Dark'],
		['solarized', 'Solarized'],
		['everforest', 'Everforest'],
		['rose-pine', 'Rosé Pine'],
		['monokai', 'Monokai'],
		['synthwave-84', "Synthwave '84"],
		['kanagawa', 'Kanagawa'],
		['matrix', 'Matrix']
	];
</script>

<Seo />

<!-- Hero -->
<section class="relative overflow-hidden">
	<div class="pointer-events-none absolute inset-0 -z-10 bg-[radial-gradient(ellipse_at_top,_var(--color-primary)_0%,_transparent_55%)] opacity-20"></div>
	<div class="mx-auto max-w-6xl px-4 pt-16 pb-10 text-center sm:pt-24">
		<img src="{base}/logo.png" alt="ctail app icon" width="112" height="112" class="mx-auto mb-6 h-28 w-28 rounded-3xl shadow-2xl" />
		<h1 class="mx-auto max-w-3xl text-4xl font-bold tracking-tight sm:text-6xl">
			The log viewer that <span class="text-primary">keeps up</span>.
		</h1>
		<p class="mx-auto mt-5 max-w-2xl text-lg text-base-content/70 sm:text-xl">
			ctail is a native macOS log tailer. Think <code class="rounded bg-base-200 px-1.5 font-mono text-[0.9em]">tail -f</code> with regex colour highlighting, tabs, search, 21 themes and an AI assistant, built to open gigabyte files instantly.
		</p>
		<div class="mt-8 flex flex-wrap items-center justify-center gap-3">
			<a href="{base}/download/" class="btn btn-primary btn-lg">
				<Icon name="apple" class="h-5 w-5" /> Download for macOS
			</a>
			<a href={site.github} class="btn btn-lg" rel="noopener" target="_blank">
				<Icon name="github" class="h-5 w-5" /> View on GitHub
			</a>
		</div>
		<p class="mt-3 text-sm text-base-content/50">Free, open source (MIT). Requires {site.minMacOS} or later.</p>
	</div>
	<div class="mx-auto max-w-6xl px-4">
		<Shot src="main-dark.webp" alt="ctail tailing a web server log with errors, warnings and timestamps highlighted" eager class="mx-auto max-w-5xl" />
	</div>
</section>

<!-- Feature grid -->
<section class="mx-auto max-w-6xl px-4 py-20">
	<h2 class="text-center text-3xl font-bold tracking-tight">Everything tail -f should have been</h2>
	<p class="mx-auto mt-3 max-w-2xl text-center text-base-content/70">Every feature is built natively in Swift and AppKit. No web view, no Electron, no waiting.</p>
	<div class="mt-12 grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
		{#each features as f}
			<a href="{base}/features/#{f.id}" class="card card-border bg-base-200/60 transition hover:border-primary/50 hover:bg-base-200">
				<div class="card-body">
					<div class="mb-1 flex h-10 w-10 items-center justify-center rounded-lg bg-primary/15 text-primary"><Icon name={f.icon} /></div>
					<h3 class="card-title text-base">{f.title}</h3>
					<p class="text-sm text-base-content/70">{f.blurb}</p>
				</div>
			</a>
		{/each}
	</div>
</section>

<!-- How it works -->
<section class="bg-base-200/60">
	<div class="mx-auto max-w-6xl px-4 py-20">
		<h2 class="text-center text-3xl font-bold tracking-tight">How it works</h2>
		<ul class="steps steps-vertical mt-10 w-full lg:steps-horizontal">
			<li class="step step-primary">
				<div class="px-4 py-2 text-left lg:text-center">
					<div class="font-semibold">Open a file</div>
					<p class="text-sm text-base-content/70">Press ⌘O, drag a file onto the Dock icon, or double-click a .log in Finder. The tail of the file appears at once, even for multi-gigabyte logs.</p>
				</div>
			</li>
			<li class="step step-primary">
				<div class="px-4 py-2 text-left lg:text-center">
					<div class="font-semibold">Rules colour it</div>
					<p class="text-sm text-base-content/70">The active profile's regex rules colour lines and matches as they render. Tweak them in the rule editor or let the AI write a profile for you.</p>
				</div>
			</li>
			<li class="step step-primary">
				<div class="px-4 py-2 text-left lg:text-center">
					<div class="font-semibold">Follow it live</div>
					<p class="text-sm text-base-content/70">New lines stream in as they are written. Scroll up to investigate and following pauses. Search, filter, or ask the AI what happened.</p>
				</div>
			</li>
		</ul>
	</div>
</section>

<!-- Highlight: huge files -->
<section class="mx-auto max-w-6xl px-4 py-20">
	<div class="grid items-center gap-10 lg:grid-cols-2">
		<div>
			<span class="badge badge-soft badge-primary">Performance</span>
			<h2 class="mt-3 text-3xl font-bold tracking-tight">Gigabyte logs, opened in a blink</h2>
			<p class="mt-4 text-base-content/70">
				ctail reads the tail of the file first and indexes line offsets in the background, so you are reading within milliseconds. The log surface is a virtualized native table: only visible rows exist, and scrollback is paged in from disk on demand. Memory stays flat whether the file is 10 KB or 10 GB, and the status bar shows you exactly how flat.
			</p>
			<ul class="mt-6 space-y-2 text-sm">
				<li class="flex gap-2"><Icon name="check" class="mt-0.5 h-4 w-4 shrink-0 text-success" /> Tail-first reads with background indexing</li>
				<li class="flex gap-2"><Icon name="check" class="mt-0.5 h-4 w-4 shrink-0 text-success" /> Virtualized rendering, windowed disk-backed scrollback</li>
				<li class="flex gap-2"><Icon name="check" class="mt-0.5 h-4 w-4 shrink-0 text-success" /> Polling with timeouts, so NFS, SMB and SSHFS mounts never hang the UI</li>
				<li class="flex gap-2"><Icon name="check" class="mt-0.5 h-4 w-4 shrink-0 text-success" /> Inode-based rotation and truncation detection</li>
			</ul>
		</div>
		<Shot src="search.webp" alt="Search bar showing ERROR matches in a large log" />
	</div>
</section>

<!-- Themes -->
<section class="bg-base-200/60">
	<div class="mx-auto max-w-6xl px-4 py-20">
		<div class="text-center">
			<span class="badge badge-soft badge-primary">Themes</span>
			<h2 class="mt-3 text-3xl font-bold tracking-tight">21 themes. Dark and light. Or your own.</h2>
			<p class="mx-auto mt-3 max-w-2xl text-base-content/70">Palettes adapted from OpenCode, each with a dark and light variant. Custom themes are one JSON file away.</p>
		</div>
		<div class="mt-10 grid grid-cols-2 gap-4 md:grid-cols-3 lg:grid-cols-4">
			{#each themes as [id, name]}
				<figure>
					<img src="{base}/screenshots/themes/{id}.webp" alt="ctail in the {name} theme" loading="lazy" decoding="async" class="rounded-box border border-base-300 shadow-md" />
					<figcaption class="mt-1.5 text-center text-xs text-base-content/60">{name}</figcaption>
				</figure>
			{/each}
		</div>
		<p class="mt-6 text-center text-sm text-base-content/60">Plus Catppuccin (Latte, Frappé, Macchiato, Mocha), Ayu, Night Owl, Cobalt2, GitHub, Palenight and Zenburn. <a href="{base}/docs/themes/" class="link">See all themes</a>.</p>
	</div>
</section>

<!-- AI -->
<section class="mx-auto max-w-6xl px-4 py-20">
	<div class="grid items-center gap-10 lg:grid-cols-2">
		<Shot src="ai.webp" alt="AI assistant summarising a log" class="order-last lg:order-first" />
		<div>
			<span class="badge badge-soft badge-primary">AI assistant</span>
			<h2 class="mt-3 text-3xl font-bold tracking-tight">Ask the log what happened</h2>
			<p class="mt-4 text-base-content/70">
				Select a stack trace and ask for an explanation, or point the assistant at an unfamiliar format and have it write a complete highlighting profile. Bring your own provider: OpenAI, Anthropic, GitHub Models, GitHub Copilot, or a local model through Ollama or LM Studio. Nothing leaves your Mac until you press Ask.
			</p>
			<a href="{base}/docs/ai-assistant/" class="btn btn-sm mt-6">Read the AI docs</a>
		</div>
	</div>
</section>

<!-- Pro -->
<section class="bg-base-200/60">
	<div class="mx-auto max-w-6xl px-4 py-20">
		<div class="text-center">
			<h2 class="text-3xl font-bold tracking-tight">Free to use. Pro when you want more.</h2>
			<p class="mx-auto mt-3 max-w-2xl text-base-content/70">The core viewer is free forever. ctail Pro is a one-time in-app purchase on the Mac App Store. No subscription, no account.</p>
		</div>
		<div class="mx-auto mt-10 grid max-w-3xl gap-4 sm:grid-cols-2">
			<div class="card card-border bg-base-100">
				<div class="card-body">
					<h3 class="card-title">Free</h3>
					<ul class="mt-2 space-y-2 text-sm">
						<li class="flex gap-2"><Icon name="check" class="h-4 w-4 shrink-0 text-success" /> Real-time tailing of files of any size</li>
						<li class="flex gap-2"><Icon name="check" class="h-4 w-4 shrink-0 text-success" /> Regex highlighting rules and profiles</li>
						<li class="flex gap-2"><Icon name="check" class="h-4 w-4 shrink-0 text-success" /> Search and filter</li>
						<li class="flex gap-2"><Icon name="check" class="h-4 w-4 shrink-0 text-success" /> Two files open at once</li>
						<li class="flex gap-2"><Icon name="check" class="h-4 w-4 shrink-0 text-success" /> Catppuccin theme, dark and light</li>
					</ul>
				</div>
			</div>
			<div class="card card-border border-primary/40 bg-base-100">
				<div class="card-body">
					<h3 class="card-title">Pro <span class="badge badge-primary badge-sm">one-time purchase</span></h3>
					<ul class="mt-2 space-y-2 text-sm">
						<li class="flex gap-2"><Icon name="check" class="h-4 w-4 shrink-0 text-success" /> Everything in Free</li>
						<li class="flex gap-2"><Icon name="check" class="h-4 w-4 shrink-0 text-success" /> Unlimited files open at once</li>
						<li class="flex gap-2"><Icon name="check" class="h-4 w-4 shrink-0 text-success" /> All 21 themes plus custom themes</li>
						<li class="flex gap-2"><Icon name="check" class="h-4 w-4 shrink-0 text-success" /> AI assistant: explain logs and generate rule profiles</li>
						<li class="flex gap-2"><Icon name="check" class="h-4 w-4 shrink-0 text-success" /> Yours forever, on every Mac signed in to your Apple ID</li>
					</ul>
				</div>
			</div>
		</div>
		<p class="mt-6 text-center text-sm text-base-content/60">Prefer to build it yourself? The source is MIT licensed on <a href={site.github} class="link" rel="noopener">GitHub</a>.</p>
	</div>
</section>

<!-- CTA -->
<section class="mx-auto max-w-6xl px-4 py-20 text-center">
	<h2 class="text-3xl font-bold tracking-tight">Stop squinting at raw logs</h2>
	<p class="mx-auto mt-3 max-w-xl text-base-content/70">Download ctail for macOS and open your biggest log file. It will be ready before you finish reading this sentence.</p>
	<div class="mt-8 flex flex-wrap justify-center gap-3">
		<a href="{base}/download/" class="btn btn-primary btn-lg"><Icon name="download" class="h-5 w-5" /> Download</a>
		<a href="{base}/docs/" class="btn btn-lg"><Icon name="book" class="h-5 w-5" /> Read the docs</a>
	</div>
</section>
