<script lang="ts">
	import { base } from '$app/paths';
	import { site } from '$lib/site';
	import Icon from '$lib/components/Icon.svelte';
	import Seo from '$lib/components/Seo.svelte';
</script>

<Seo title="Download" description="Download ctail for macOS from the Mac App Store, join the TestFlight beta, or get the cross-platform builds for Linux and Windows from GitHub." />

<div class="mx-auto max-w-6xl px-4 py-16">
	<h1 class="text-4xl font-bold tracking-tight sm:text-5xl">Download</h1>
	<p class="mt-4 max-w-2xl text-lg text-base-content/70">ctail for macOS is a native app distributed through the Mac App Store. The original cross-platform edition for Linux and Windows is on GitHub.</p>

	<div class="mt-12 grid gap-6 lg:grid-cols-3">
		<div class="card card-border bg-base-200/60 lg:col-span-2">
			<div class="card-body">
				<div class="flex items-center gap-4">
					<img src="{base}/logo-256.png" alt="" width="64" height="64" class="h-16 w-16 rounded-2xl shadow-lg" />
					<div>
						<h2 class="card-title text-2xl">ctail for macOS</h2>
						<p class="text-sm text-base-content/60">Version {site.version} · Requires {site.minMacOS} or later · Apple silicon and Intel</p>
					</div>
				</div>
				<p class="mt-4 text-base-content/70">Free download. Sandboxed, notarised and updated automatically through the App Store. ctail Pro is available as a one-time in-app purchase.</p>
				<div class="card-actions mt-4 items-center gap-3">
					<a href={site.appStore} class="btn btn-primary btn-lg" rel="noopener"><Icon name="apple" class="h-5 w-5" /> Download on the Mac App Store</a>
				</div>
				<div role="alert" class="alert alert-soft alert-info mt-6">
					<Icon name="sparkles" class="h-5 w-5" />
					<div>
						<div class="font-semibold">Beta builds via TestFlight</div>
						<div class="text-sm">Want new features first? Beta builds ship through TestFlight ahead of each App Store release. Pro is free to try in TestFlight. Ask for an invite in the <a href={site.discussions} class="link" rel="noopener">GitHub discussions</a>.</div>
					</div>
				</div>
			</div>
		</div>

		<div class="card card-border bg-base-200/60">
			<div class="card-body">
				<h2 class="card-title">What you get</h2>
				<ul class="mt-2 space-y-2 text-sm">
					<li class="flex gap-2"><Icon name="check" class="mt-0.5 h-4 w-4 shrink-0 text-success" /> Real-time tailing with rotation detection</li>
					<li class="flex gap-2"><Icon name="check" class="mt-0.5 h-4 w-4 shrink-0 text-success" /> Instant open for multi-gigabyte files</li>
					<li class="flex gap-2"><Icon name="check" class="mt-0.5 h-4 w-4 shrink-0 text-success" /> Regex highlighting rules and profiles</li>
					<li class="flex gap-2"><Icon name="check" class="mt-0.5 h-4 w-4 shrink-0 text-success" /> Search with filter mode</li>
					<li class="flex gap-2"><Icon name="check" class="mt-0.5 h-4 w-4 shrink-0 text-success" /> Tabs, session restore, Finder integration</li>
					<li class="flex gap-2"><Icon name="sparkles" class="mt-0.5 h-4 w-4 shrink-0 text-primary" /> Pro: unlimited tabs, all themes, AI assistant</li>
				</ul>
				<a href="{base}/docs/pro/" class="link mt-3 text-sm">Free vs Pro in detail</a>
			</div>
		</div>
	</div>

	<h2 class="mt-20 text-2xl font-bold tracking-tight">Build from source</h2>
	<p class="mt-2 max-w-2xl text-base-content/70">The macOS app is a Swift package. With Xcode or the Command Line Tools installed, this builds and runs the debug binary:</p>
	<pre class="mt-4 overflow-x-auto rounded-box bg-base-200 p-4 text-sm"><code>git clone {site.github}.git
cd ctail/macos
swift build
./.build/debug/ctailmac /var/log/system.log</code></pre>
	<p class="mt-2 text-sm text-base-content/60">Use <code class="rounded bg-base-200 px-1 font-mono">make bundle</code> for a distributable <code class="rounded bg-base-200 px-1 font-mono">ctail.app</code>. Source builds have no App Store licensing, so Pro features can be enabled from the app menu in debug builds.</p>

	<h2 class="mt-20 text-2xl font-bold tracking-tight">Linux and Windows</h2>
	<p class="mt-2 max-w-2xl text-base-content/70">The cross-platform edition of ctail (Go and Svelte, built with Wails) runs on Linux, Windows and macOS and shares the same highlighting rules, profiles and themes. Grab the latest release from GitHub.</p>
	<div class="mt-6 grid gap-4 sm:grid-cols-3">
		<div class="card card-border bg-base-200/60">
			<div class="card-body">
				<h3 class="card-title text-base"><Icon name="terminal" class="h-5 w-5" /> Linux</h3>
				<p class="text-sm text-base-content/70">.deb and .rpm packages, plus a standalone binary. Needs GTK 3 and WebKit2GTK 4.1.</p>
				<a href={site.latestRelease} class="btn btn-sm mt-2" rel="noopener">Latest release</a>
			</div>
		</div>
		<div class="card card-border bg-base-200/60">
			<div class="card-body">
				<h3 class="card-title text-base"><Icon name="terminal" class="h-5 w-5" /> Windows</h3>
				<p class="text-sm text-base-content/70">A single .exe. Uses the WebView2 runtime that ships with Windows 10 and 11.</p>
				<a href={site.latestRelease} class="btn btn-sm mt-2" rel="noopener">Latest release</a>
			</div>
		</div>
		<div class="card card-border bg-base-200/60">
			<div class="card-body">
				<h3 class="card-title text-base"><Icon name="terminal" class="h-5 w-5" /> macOS (Wails)</h3>
				<p class="text-sm text-base-content/70">A universal binary of the cross-platform edition, unsigned. Most Mac users should prefer the native app above.</p>
				<a href={site.latestRelease} class="btn btn-sm mt-2" rel="noopener">Latest release</a>
			</div>
		</div>
	</div>
	<p class="mt-4 text-sm text-base-content/60">Release notes and older versions: <a href={site.releases} class="link" rel="noopener">github.com/bisand/ctail/releases</a>.</p>
</div>
