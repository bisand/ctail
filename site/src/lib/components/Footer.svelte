<script lang="ts">
	import { base } from '$app/paths';
	import { site, nav } from '$lib/site';
</script>

<footer class="mt-24 border-t border-base-300 bg-base-200">
	<div class="footer mx-auto max-w-6xl px-4 py-12 sm:footer-horizontal">
		<aside>
			<div class="flex items-center gap-2 text-lg font-semibold">
				<img src="{base}/logo-256.png" alt="" width="32" height="32" class="rounded-md" />
				{site.name}
			</div>
			<p class="max-w-xs text-base-content/70">{site.tagline}<br />Made in Norway by <a href={site.authorUrl} class="link">{site.author}</a>. MIT licensed.</p>
		</aside>
		<nav>
			<h6 class="footer-title">Site</h6>
			{#each nav as item}
				<a href="{base}{item.href}" class="link link-hover">{item.label}</a>
			{/each}
		</nav>
		<nav>
			<h6 class="footer-title">Project</h6>
			<a href={site.github} class="link link-hover" rel="noopener">GitHub repository</a>
			<a href={site.releases} class="link link-hover" rel="noopener">Releases</a>
			<a href={site.issues} class="link link-hover" rel="noopener">Issue tracker</a>
			<a href={site.security} class="link link-hover" rel="noopener">Security policy</a>
		</nav>
		<nav>
			<h6 class="footer-title">Docs</h6>
			<a href="{base}/docs/getting-started/" class="link link-hover">Getting started</a>
			<a href="{base}/docs/highlighting-rules/" class="link link-hover">Highlighting rules</a>
			<a href="{base}/docs/ai-assistant/" class="link link-hover">AI assistant</a>
			<a href="{base}/docs/keyboard-shortcuts/" class="link link-hover">Keyboard shortcuts</a>
		</nav>
	</div>
	<div class="mx-auto max-w-6xl px-4 pb-8 text-xs text-base-content/50">
		© 2024–2026 {site.author}. Theme palettes adapted from <a href="https://github.com/anomalyco/opencode" class="link" rel="noopener">OpenCode</a>. Apple, macOS, Mac App Store and TestFlight are trademarks of Apple Inc.
	</div>
</footer>
