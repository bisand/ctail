<script lang="ts">
	import { page } from '$app/state';
	import { base } from '$app/paths';
	import { docPages } from '$lib/site';
	let { children } = $props();
	const groups = [...new Set(docPages.map((p) => p.group))];
	const current = $derived(page.url.pathname);
	const isCurrent = (slug: string) => current === `${base}/docs/${slug}/`;
</script>

<div class="mx-auto max-w-6xl px-4 py-10 lg:grid lg:grid-cols-[220px_1fr] lg:gap-10">
	<aside class="mb-8 lg:mb-0">
		<details class="lg:hidden" open={false}>
			<summary class="btn btn-block btn-sm justify-between">Documentation menu</summary>
			<ul class="menu menu-sm mt-2 w-full rounded-box bg-base-200 p-2">
				{#each groups as g}
					<li class="menu-title">{g}</li>
					{#each docPages.filter((p) => p.group === g) as p}
						<li><a href="{base}/docs/{p.slug}/" class:menu-active={isCurrent(p.slug)}>{p.title}</a></li>
					{/each}
				{/each}
			</ul>
		</details>
		<div class="sticky top-20 hidden max-h-[calc(100vh-6rem)] overflow-y-auto lg:block">
			<ul class="menu menu-sm w-full p-0">
				<li><a href="{base}/docs/" class="font-semibold" class:menu-active={current === `${base}/docs/`}>Overview</a></li>
				{#each groups as g}
					<li class="menu-title">{g}</li>
					{#each docPages.filter((p) => p.group === g) as p}
						<li><a href="{base}/docs/{p.slug}/" class:menu-active={isCurrent(p.slug)}>{p.title}</a></li>
					{/each}
				{/each}
			</ul>
		</div>
	</aside>
	<div class="min-w-0">
		{@render children()}
	</div>
</div>
