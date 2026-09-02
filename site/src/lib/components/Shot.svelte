<script lang="ts">
	import { base } from '$app/paths';
	let {
		src,
		alt,
		caption = '',
		class: cls = '',
		eager = false
	}: { src: string; alt: string; caption?: string; class?: string; eager?: boolean } = $props();
</script>

<figure class={cls}>
	<img
		src="{base}/screenshots/{src}"
		{alt}
		loading={eager ? 'eager' : 'lazy'}
		decoding="async"
		class="w-full rounded-box border border-base-300 shadow-xl"
	/>
	{#if caption}
		<figcaption class="mt-2 text-center text-sm text-base-content/60">{caption}</figcaption>
	{/if}
</figure>
