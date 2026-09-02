<script lang="ts">
	import { page } from '$app/state';
	import { base } from '$app/paths';
	import { nav, site } from '$lib/site';
	import ThemeToggle from './ThemeToggle.svelte';

	const isActive = (href: string) => page.url.pathname.startsWith(base + href.replace(/\/$/, ''));
</script>

<header class="sticky top-0 z-40 border-b border-base-300 bg-base-100/80 backdrop-blur">
	<div class="navbar mx-auto max-w-6xl px-4">
		<div class="navbar-start">
			<a href="{base}/" class="flex items-center gap-2 text-lg font-semibold">
				<img src="{base}/logo-256.png" alt="" width="28" height="28" class="rounded-md" />
				{site.name}
			</a>
		</div>
		<nav class="navbar-center hidden md:flex">
			<ul class="menu menu-horizontal gap-1 px-1">
				{#each nav as item}
					<li>
						<a href="{base}{item.href}" class:menu-active={isActive(item.href)}>{item.label}</a>
					</li>
				{/each}
			</ul>
		</nav>
		<div class="navbar-end gap-1">
			<ThemeToggle />
			<a href={site.github} class="btn btn-ghost btn-sm" rel="noopener" target="_blank" aria-label="GitHub">
				<svg viewBox="0 0 16 16" width="18" height="18" fill="currentColor" aria-hidden="true"><path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z"/></svg>
				<span class="hidden sm:inline">GitHub</span>
			</a>
			<a href="{base}/download/" class="btn btn-primary btn-sm hidden sm:inline-flex">Download</a>
			<details class="dropdown dropdown-end md:hidden">
				<summary class="btn btn-ghost btn-square btn-sm" aria-label="Menu">
					<svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M4 6h16M4 12h16M4 18h16"/></svg>
				</summary>
				<ul class="dropdown-content menu z-50 mt-2 w-48 rounded-box border border-base-300 bg-base-100 p-2 shadow-lg">
					{#each nav as item}
						<li><a href="{base}{item.href}">{item.label}</a></li>
					{/each}
				</ul>
			</details>
		</div>
	</div>
</header>
