<script lang="ts">
	import { base } from '$app/paths';
	import { site } from '$lib/site';
	import Icon from '$lib/components/Icon.svelte';
	import Seo from '$lib/components/Seo.svelte';

	const faq = [
		{
			q: 'Is ctail free?',
			a: 'Yes. The viewer is free and the source is MIT licensed. ctail Pro is an optional one-time in-app purchase on the Mac App Store that unlocks unlimited tabs, all themes and the AI assistant.'
		},
		{
			q: 'Does it handle really large files?',
			a: 'That is the whole point. ctail reads the tail first, indexes line offsets in the background and renders only the rows on screen. Multi-gigabyte files open instantly and memory stays flat while you scroll.'
		},
		{
			q: 'My file is on a network share and it is slow. Is that a bug?',
			a: 'Network mounts are supported and polled rather than watched, so a slow share never freezes the app. If the share is unreachable the tab shows a warning marker until it comes back. You can raise the poll interval and read timeout in Settings.'
		},
		{
			q: 'Why can I only open two files?',
			a: 'The free tier allows two tabs at once. ctail Pro removes the limit. If you built ctail from source, debug builds can enable Pro from the app menu.'
		},
		{
			q: 'Where does the AI send my logs?',
			a: 'Only to the provider you configured, and only when you press Ask or Generate Rules Profile. With a local server such as Ollama or LM Studio the data never leaves your Mac. API keys are stored in your local settings file.'
		},
		{
			q: 'Can I use my own colour theme?',
			a: 'Yes. Drop a JSON file into the themes folder in Application Support and it appears in Settings. See the custom themes guide in the docs.'
		},
		{
			q: 'Is there a Linux or Windows version?',
			a: 'Yes. The original cross-platform edition, written in Go and Svelte, is on the download page. Rules, profiles and themes are compatible between the two.'
		}
	];
</script>

<Seo title="Support" description="Get help with ctail: frequently asked questions, bug reports, feature requests, security disclosures and contact details." />

<div class="mx-auto max-w-6xl px-4 py-16">
	<h1 class="text-4xl font-bold tracking-tight sm:text-5xl">Support</h1>
	<p class="mt-4 max-w-2xl text-lg text-base-content/70">ctail is developed in the open. Most questions are answered fastest on GitHub, where the maintainer and other users can see them.</p>

	<div class="mt-12 grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
		<a href="{base}/docs/" class="card card-border bg-base-200/60 transition hover:border-primary/50">
			<div class="card-body">
				<Icon name="book" class="h-7 w-7 text-primary" />
				<h2 class="card-title text-base">User manual</h2>
				<p class="text-sm text-base-content/70">Every feature, setting and shortcut, plus troubleshooting.</p>
			</div>
		</a>
		<a href={site.issues} rel="noopener" class="card card-border bg-base-200/60 transition hover:border-primary/50">
			<div class="card-body">
				<Icon name="github" class="h-7 w-7 text-primary" />
				<h2 class="card-title text-base">Report a bug</h2>
				<p class="text-sm text-base-content/70">Open an issue on GitHub. Include your macOS version and steps to reproduce.</p>
			</div>
		</a>
		<a href={site.discussions} rel="noopener" class="card card-border bg-base-200/60 transition hover:border-primary/50">
			<div class="card-body">
				<Icon name="chat" class="h-7 w-7 text-primary" />
				<h2 class="card-title text-base">Ask a question</h2>
				<p class="text-sm text-base-content/70">GitHub Discussions for questions, ideas and feature requests.</p>
			</div>
		</a>
		<a href={site.security} rel="noopener" class="card card-border bg-base-200/60 transition hover:border-primary/50">
			<div class="card-body">
				<Icon name="shield" class="h-7 w-7 text-primary" />
				<h2 class="card-title text-base">Security</h2>
				<p class="text-sm text-base-content/70">Found a vulnerability? Please follow the security policy for private disclosure.</p>
			</div>
		</a>
	</div>

	<h2 class="mt-20 text-2xl font-bold tracking-tight">Frequently asked questions</h2>
	<div class="mt-6 space-y-2">
		{#each faq as item, i}
			<div class="collapse collapse-arrow border border-base-300 bg-base-200/60">
				<input type="radio" name="faq" checked={i === 0} />
				<div class="collapse-title font-semibold">{item.q}</div>
				<div class="collapse-content text-sm text-base-content/80"><p>{item.a}</p></div>
			</div>
		{/each}
	</div>

	<h2 class="mt-20 text-2xl font-bold tracking-tight">Writing a good bug report</h2>
	<ol class="mt-4 list-decimal space-y-2 pl-6 text-base-content/80">
		<li>Say which edition you use: the native macOS app from the App Store or TestFlight, or the cross-platform build. The version is in About ctail.</li>
		<li>Describe what you did, what you expected and what happened instead. Screenshots help.</li>
		<li>If a specific file triggers it, note its size and where it lives, for example a local SSD or an SMB share.</li>
		<li>For highlighting problems, paste the rule pattern and one log line that should match.</li>
	</ol>

	<h2 class="mt-20 text-2xl font-bold tracking-tight">Contributing</h2>
	<p class="mt-4 max-w-2xl text-base-content/80">Pull requests are welcome. The repository has a code of conduct, a security policy and an issue tracker. The macOS app lives in the <code class="rounded bg-base-200 px-1 font-mono">macos/</code> folder and builds with <code class="rounded bg-base-200 px-1 font-mono">swift build</code>. Run <code class="rounded bg-base-200 px-1 font-mono">make test</code> there for the self-test suite before opening a PR.</p>
	<a href={site.github} class="btn mt-6" rel="noopener"><Icon name="github" class="h-5 w-5" /> github.com/bisand/ctail</a>
</div>
