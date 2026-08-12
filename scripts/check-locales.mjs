// Fails if any locale drifts from English: a missing key silently falls back
// to English at runtime, which is exactly the kind of rot nobody notices.
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const dir = resolve(dirname(fileURLToPath(import.meta.url)), "../src/i18n/locales");

/** `files_one` and `files_many` are the same message in different plural forms. */
const base = (key) => key.replace(/_(zero|one|two|few|many|other)$/, "");

const load = (file) => Object.keys(JSON.parse(readFileSync(join(dir, file), "utf8")));
const expected = new Set(load("en.json").map(base));

let failed = false;
for (const file of readdirSync(dir).filter((name) => name.endsWith(".json"))) {
  const keys = load(file);
  const present = new Set(keys.map(base));
  const missing = [...expected].filter((key) => !present.has(key));
  const extra = [...present].filter((key) => !expected.has(key));

  const problems = [
    missing.length && `missing ${missing.join(", ")}`,
    extra.length && `unknown ${extra.join(", ")}`,
  ].filter(Boolean);

  if (problems.length) {
    failed = true;
    console.error(`✗ ${file}: ${problems.join("; ")}`);
  } else {
    console.log(`✓ ${file} (${keys.length} keys)`);
  }
}

if (failed) {
  console.error("\nLocale files are out of sync with en.json.");
  process.exit(1);
}
