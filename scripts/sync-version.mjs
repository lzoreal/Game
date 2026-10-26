import fs from 'node:fs';

const raw = process.argv[2] || '';
const match = raw.match(/^v?(\d+\.\d+\.\d+)$/);
if (!match) {
  console.error(`invalid version tag: ${raw}`);
  process.exit(1);
}
const version = match[1];

function patch(file, pattern, replacement) {
  const text = fs.readFileSync(file, 'utf8');
  if (!pattern.test(text)) {
    throw new Error(`pattern not found in ${file}`);
  }
  fs.writeFileSync(file, text.replace(pattern, replacement));
}

patch('src-tauri/tauri.conf.json', /"version":\s*"[^"]+"/, `"version": "${version}"`);
patch('src-tauri/Cargo.toml', /^version\s*=\s*"[^"]+"/m, `version = "${version}"`);
patch(
  'src-tauri/src/state.rs',
  /pub const APP_VERSION: &str = "v[^"]+";/,
  `pub const APP_VERSION: &str = "v${version}";`
);
console.log(`version -> ${version}`);
