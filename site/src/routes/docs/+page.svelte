<script lang="ts">
	import { base } from '$app/paths';
	import { docPages } from '$lib/site';
	import Seo from '$lib/components/Seo.svelte';
	const groups = [...new Set(docPages.map((p) => p.group))];
</script>

<Seo title="Documentation" description="The ctail user manual: getting started, tabs, following, search, highlighting rules, profiles, themes, the AI assistant, settings, shortcuts and troubleshooting." />

<h1 class="text-4xl font-bold tracking-tight">User manual</h1>
<p class="mt-3 text-lg text-base-content/70">Everything about ctail for macOS. Start with Getting started if you are new, or jump straight to a topic.</p>

<div class="prose-docs mt-8 rounded-box border border-base-300 bg-base-200/60 p-6">
	<h2 class="mt-0!">Quick start</h2>
	<ol>
		<li>Install ctail from the <a href="{base}/download/">Mac App Store</a> and launch it.</li>
		<li>Press <kbd>⌘O</kbd> and pick a log file, or drop one on the Dock icon. The end of the file appears immediately.</li>
		<li>Tick <strong>Follow</strong> in the status bar (it is on by default) and new lines stream in as they are written.</li>
		<li>Press <kbd>⌘F</kbd> to search, or open <strong>View ▸ Profiles &amp; Rules…</strong> to change how lines are coloured.</li>
	</ol>
</div>

{#each groups as g}
	<h2 class="mt-10 text-xl font-semibold">{g}</h2>
	<div class="mt-3 grid gap-3 sm:grid-cols-2">
		{#each docPages.filter((p) => p.group === g) as p}
			<a href="{base}/docs/{p.slug}/" class="card card-border card-sm bg-base-200/40 transition hover:border-primary/50">
				<div class="card-body"><span class="card-title text-base">{p.title}</span></div>
			</a>
		{/each}
	</div>
{/each}
