import fs from 'node:fs';
import path from 'node:path';

const dir = process.argv[2] || 'sources';

const files = fs
  .readdirSync(dir, { withFileTypes: true })
  .filter((entry) => entry.isFile() && entry.name.endsWith('.source-url'))
  .map((entry) => entry.name)
  .sort();

const urls = [];
for (const name of files) {
  const text = fs.readFileSync(path.join(dir, name), 'utf8');
  for (const line of text.split(/\r?\n/)) {
    const url = line.trim();
    if (url) urls.push(url);
  }
}

process.stdout.write(urls.map((url) => `${url}\n`).join(''));
