import { readFile } from 'node:fs/promises';
import { prune, version } from 'polyfill-prune';

const source = await readFile(new URL('./vendor/index.js', import.meta.url), 'utf8');
const options = { targets: ['chrome100', 'safari15.4'], filename: 'vendor.js' };
console.log(JSON.stringify({ version: version(), ...(await prune(source, options)) }, null, 2));
