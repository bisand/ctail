import { error } from '@sveltejs/kit';
import { docPages } from '$lib/site';
import type { EntryGenerator, PageLoad } from './$types';

const modules = import.meta.glob('/src/lib/docs/*.md');

export const entries: EntryGenerator = () => docPages.map((p) => ({ slug: p.slug }));

export const load: PageLoad = async ({ params }) => {
	const meta = docPages.find((p) => p.slug === params.slug);
	const loader = modules[`/src/lib/docs/${params.slug}.md`];
	if (!meta || !loader) error(404, 'Not found');
	const mod = (await loader()) as { default: unknown };
	const idx = docPages.indexOf(meta);
	return {
		meta,
		component: mod.default,
		prev: idx > 0 ? docPages[idx - 1] : null,
		next: idx < docPages.length - 1 ? docPages[idx + 1] : null
	};
};
