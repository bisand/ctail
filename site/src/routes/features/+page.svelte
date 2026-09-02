<script lang="ts">
	import { base } from '$app/paths';
	import { features } from '$lib/features';
	import Icon from '$lib/components/Icon.svelte';
	import Shot from '$lib/components/Shot.svelte';
	import Seo from '$lib/components/Seo.svelte';
</script>

<Seo title="Features" description="Every ctail feature explained: real-time tailing, huge-file performance, regex highlighting, profiles, search and filter, tabs, themes, the AI assistant and native macOS integration." />

<div class="mx-auto max-w-6xl px-4 py-16">
	<div class="max-w-3xl">
		<h1 class="text-4xl font-bold tracking-tight sm:text-5xl">Features</h1>
		<p class="mt-4 text-lg text-base-content/70">A feature-by-feature tour of ctail for macOS. Each section links to the relevant part of the user manual.</p>
	</div>
	<nav class="mt-8 flex flex-wrap gap-2">
		{#each features as f}
			<a href="#{f.id}" class="btn btn-sm btn-soft"><Icon name={f.icon} class="h-4 w-4" /> {f.title}</a>
		{/each}
	</nav>

	<div class="mt-16 space-y-24">
		{#each features as f, i}
			<section id={f.id} class="scroll-mt-24">
				<div class="grid items-start gap-10 {f.shot ? 'lg:grid-cols-2' : ''}">
					<div class={f.shot && i % 2 === 1 ? 'lg:order-last' : ''}>
						<div class="flex h-11 w-11 items-center justify-center rounded-lg bg-primary/15 text-primary"><Icon name={f.icon} /></div>
						<h2 class="mt-4 text-2xl font-bold tracking-tight sm:text-3xl">{f.title}</h2>
						<p class="mt-3 text-base-content/70">{f.blurb}</p>
						<ul class="mt-5 space-y-2.5 text-sm">
							{#each f.details as d}
								<li class="flex gap-2.5"><Icon name="check" class="mt-0.5 h-4 w-4 shrink-0 text-success" /><span>{d}</span></li>
							{/each}
						</ul>
						<a href="{base}/docs/{f.doc}/" class="btn btn-sm mt-6"><Icon name="book" class="h-4 w-4" /> Read the manual</a>
					</div>
					{#if f.shot}
						<Shot src={f.shot} light={f.shotLight} alt={f.shotAlt ?? f.title} />
					{/if}
				</div>
			</section>
		{/each}
	</div>

	<section class="mt-24 rounded-box border border-base-300 bg-base-200/60 p-8 text-center">
		<h2 class="text-2xl font-bold tracking-tight">Ready to try it?</h2>
		<p class="mt-2 text-base-content/70">Free on the Mac App Store. Pro is a one-time purchase.</p>
		<a href="{base}/download/" class="btn btn-primary mt-6"><Icon name="apple" class="h-5 w-5" /> Download for macOS</a>
	</section>
</div>
