<script lang="ts">
	import { site } from '$lib/site';
	import Seo from '$lib/components/Seo.svelte';
</script>

<Seo title="Privacy policy" description="ctail's privacy policy: the app collects no data. Log content is sent only to the AI provider you configure, and only when you ask." />

<div class="prose-docs mx-auto max-w-3xl px-4 py-16">
	<h1 class="text-4xl font-bold tracking-tight">Privacy policy</h1>
	<p class="text-base-content/60">Last updated 3 September 2026</p>

	<h2>Summary</h2>
	<p>ctail does not collect, store or transmit any personal data. There are no accounts, no analytics, no telemetry and no advertising. Everything the app does happens on your Mac, with two narrow exceptions described below that you control.</p>

	<h2>What stays on your Mac</h2>
	<ul>
		<li>The log files you open are read from disk and displayed. They are never uploaded anywhere by the app itself.</li>
		<li>Settings, highlighting profiles, custom themes, recent files and window state are stored in <code>~/Library/Application Support/ctail/</code>.</li>
		<li>The App Store build runs in the macOS sandbox and only reaches files you explicitly open or drop on the app. It keeps security-scoped bookmarks so those files can be reopened after a relaunch.</li>
	</ul>

	<h2>The AI assistant (optional, Pro)</h2>
	<p>If you configure an AI provider and press <strong>Ask</strong> or <strong>Generate Rules Profile</strong>, the log text you chose to include and your question are sent to that provider's API endpoint. Nothing is sent automatically or in the background.</p>
	<ul>
		<li>Which provider receives the data is entirely your choice: OpenAI, Anthropic, GitHub Models, GitHub Copilot, or any OpenAI-compatible server such as Ollama or LM Studio running locally. With a local server, nothing leaves your Mac.</li>
		<li>Data sent to a third-party provider is handled under that provider's own privacy terms.</li>
		<li>API keys and tokens are stored in your local settings file and are only ever sent to the endpoint you configured.</li>
	</ul>

	<h2>Update check (optional)</h2>
	<p>Source builds of ctail can check GitHub's public releases API for a newer version. The request contains no personal data beyond what any HTTPS request carries. App Store builds are updated by the App Store, and the check can be disabled in Settings.</p>

	<h2>In-app purchase</h2>
	<p>ctail Pro is sold through Apple's in-app purchase system. Payment is handled entirely by Apple. ctail never sees your payment details, and only receives a receipt confirming the purchase.</p>

	<h2>Children</h2>
	<p>ctail is a developer tool and is not directed at children. It collects no data from anyone.</p>

	<h2>Changes and contact</h2>
	<p>If this policy changes, the new version will be published at this address with an updated date. Questions can be raised on the <a href={site.issues} rel="noopener">GitHub issue tracker</a> or through the <a href="../support/">support page</a>.</p>
</div>
