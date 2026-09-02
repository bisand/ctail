<script lang="ts">
	import { onMount } from 'svelte';
	let dark = $state(true);
	onMount(() => {
		const attr = document.documentElement.getAttribute('data-theme');
		dark = attr ? attr === 'ctail-dark' : window.matchMedia('(prefers-color-scheme: dark)').matches;
	});
	function toggle() {
		dark = !dark;
		const t = dark ? 'ctail-dark' : 'ctail-light';
		document.documentElement.setAttribute('data-theme', t);
		try {
			localStorage.setItem('ctail-theme', t);
		} catch {}
	}
</script>

<button class="btn btn-ghost btn-square btn-sm" onclick={toggle} aria-label="Toggle dark mode">
	{#if dark}
		<svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><circle cx="12" cy="12" r="4"/><path d="M12 2v2m0 16v2M4.9 4.9l1.4 1.4m11.4 11.4 1.4 1.4M2 12h2m16 0h2M4.9 19.1l1.4-1.4m11.4-11.4 1.4-1.4"/></svg>
	{:else}
		<svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z"/></svg>
	{/if}
</button>
