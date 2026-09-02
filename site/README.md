# ctail website

Marketing site and user manual for ctail, built with SvelteKit, Tailwind CSS 4 and daisyUI.
Fully prerendered to static HTML (`@sveltejs/adapter-static`), so it can be hosted anywhere.

```bash
npm install
npm run dev        # http://localhost:5173
npm run build      # static output in build/
npm run preview
```

- `src/routes/` — pages (home, features, download, support, docs)
- `src/lib/docs/*.md` — the user manual, one Markdown file per page (mdsvex). Register new pages in `src/lib/site.ts`.
- `src/lib/features.ts` — feature copy shared by the home page and the features page
- `static/screenshots/` — app screenshots. Regenerate with the debug-only `CTAIL_DEBUG_SHOW` hook in the macOS app (see `macos/Sources/ctailmac/AppDelegate.swift`).

Deployed to GitHub Pages by `.github/workflows/site.yml`. Set `BASE_PATH` when the site is served from a sub-path.
