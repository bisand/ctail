<script lang="ts">
	import { base } from '$app/paths';
	/**
	 * A screenshot that follows the site theme. `src` is the dark-mode image; the
	 * light-mode image defaults to `<name>-light.<ext>` and can be overridden with
	 * `light`. Pass `themed={false}` for a single image in both modes.
	 */
	let {
		src,
		alt,
		light,
		themed = true,
		caption = '',
		class: cls = '',
		eager = false
	}: {
		src: string;
		alt: string;
		light?: string;
		themed?: boolean;
		caption?: string;
		class?: string;
		eager?: boolean;
	} = $props();
	const lightSrc = $derived(light ?? src.replace(/(\.[a-z0-9]+)$/i, '-light$1'));
	const loading = $derived(eager ? 'eager' : 'lazy');
</script>

<figure class={cls}>
	{#if themed}
		<img src="{base}/screenshots/{src}" {alt} {loading} decoding="async" class="shot-dark w-full rounded-box border border-base-300 shadow-xl" />
		<img src="{base}/screenshots/{lightSrc}" {alt} {loading} decoding="async" class="shot-light w-full rounded-box border border-base-300 shadow-xl" />
	{:else}
		<img src="{base}/screenshots/{src}" {alt} {loading} decoding="async" class="w-full rounded-box border border-base-300 shadow-xl" />
	{/if}
	{#if caption}
		<figcaption class="mt-2 text-center text-sm text-base-content/60">{caption}</figcaption>
	{/if}
</figure>
