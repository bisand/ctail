import adapter from '@sveltejs/adapter-static';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';
import { mdsvex } from 'mdsvex';

/** @type {import('@sveltejs/kit').Config} */
const config = {
	extensions: ['.svelte', '.md'],
	preprocess: [vitePreprocess(), mdsvex({ extensions: ['.md'] })],
	kit: {
		adapter: adapter({ pages: 'build', assets: 'build', strict: true }),
		paths: { base: process.env.BASE_PATH ?? '' },
		prerender: { entries: ['*'] }
	}
};

export default config;
