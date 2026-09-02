<script lang="ts">
	import { base } from '$app/paths';
	import Seo from '$lib/components/Seo.svelte';
	let { data } = $props();
	const Doc = $derived(data.component as import('svelte').Component);
</script>

<Seo title={data.meta.title} description="ctail user manual: {data.meta.title}." />

<div class="breadcrumbs text-sm text-base-content/60">
	<ul>
		<li><a href="{base}/docs/">Docs</a></li>
		<li>{data.meta.group}</li>
		<li>{data.meta.title}</li>
	</ul>
</div>
<article class="prose-docs max-w-3xl">
	<h1 class="mb-6 text-3xl font-bold tracking-tight sm:text-4xl">{data.meta.title}</h1>
	<Doc />
</article>
<nav class="mt-14 flex max-w-3xl justify-between gap-4 border-t border-base-300 pt-6 text-sm">
	<div>
		{#if data.prev}
			<a href="{base}/docs/{data.prev.slug}/" class="link link-hover">← {data.prev.title}</a>
		{/if}
	</div>
	<div class="text-right">
		{#if data.next}
			<a href="{base}/docs/{data.next.slug}/" class="link link-hover">{data.next.title} →</a>
		{/if}
	</div>
</nav>
