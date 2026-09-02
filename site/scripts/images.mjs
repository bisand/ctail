// Convert static/screenshots/**/*.png to WebP (theme thumbnails downscaled) and
// remove the PNG originals. Run: node scripts/images.mjs
import sharp from 'sharp';
import { readdir, unlink } from 'node:fs/promises';
import { join, extname } from 'node:path';

async function walk(dir) {
	for (const e of await readdir(dir, { withFileTypes: true })) {
		const p = join(dir, e.name);
		if (e.isDirectory()) await walk(p);
		else if (extname(p) === '.png') {
			const out = p.replace(/\.png$/, '.webp');
			let img = sharp(p);
			if (dir.includes('themes')) img = img.resize({ width: 900 });
			await img.webp({ quality: 88, effort: 6 }).toFile(out);
			await unlink(p);
			console.log('→', out);
		}
	}
}
await walk('static/screenshots');
