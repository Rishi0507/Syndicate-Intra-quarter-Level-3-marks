// Checks that the browser's path through the kernel gives the same answer as
// the native program.
//
// The native export writes a handful of loans' daily estimates at full
// precision. This script loads the same static files the site loads, runs
// the site's own code (bundled on the fly) against the WebAssembly module,
// and compares. Marks travel to the browser in single precision, so
// agreement is to about seven significant figures, not sixteen.
//
//   node scripts/check-kernel.mjs [build-dir]
//
// Without an argument the build directory is the one the site's data was
// exported from: real filings or the generated universe.

import { build } from "esbuild";
import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const web = resolve(here, "..");
const source = JSON.parse(readFileSync(join(web, "public", "data", "meta.json"), "utf8")).source;
const buildDir = resolve(process.argv[2] ?? join(web, "..", "data", ...(source === "synthetic" ? ["synth", "build"] : ["build"])));
const TOLERANCE = 2e-6;

const bundle = join(web, "node_modules", ".cache", "check-kernel.mjs");
await build({ entryPoints: [join(here, "check-entry.ts")], bundle: true, format: "esm", platform: "node", outfile: bundle, logLevel: "error" });
const { assembleUniverse, Kernel, computeLoan } = await import(pathToFileURL(bundle).href);

const data = join(web, "public", "data");
const bytes = (name) => {
  const b = readFileSync(join(data, name));
  return b.buffer.slice(b.byteOffset, b.byteOffset + b.byteLength);
};
const u = assembleUniverse(
  JSON.parse(readFileSync(join(data, "meta.json"), "utf8")),
  JSON.parse(readFileSync(join(data, "loans.json"), "utf8")),
  bytes("marks_i.bin"),
  bytes("marks_f.bin"),
  bytes("path.bin"),
);
const kernel = await Kernel.fromBytes(readFileSync(join(web, "public", "syndicate_kernel.wasm")));
const reference = JSON.parse(readFileSync(join(buildDir, "reference.json"), "utf8"));

let worst = 0;
let days = 0;
for (const ref of reference) {
  const lc = computeLoan(u, kernel, ref.loan);
  if (lc.start !== ref.start || lc.curve.mean.length !== ref.mean.length) {
    console.error(`loan ${ref.loan}: run covers ${lc.start}+${lc.curve.mean.length} days, native ${ref.start}+${ref.mean.length}`);
    process.exit(1);
  }
  let gap = 0;
  for (let i = 0; i < ref.mean.length; i++) {
    gap = Math.max(gap, Math.abs(lc.curve.mean[i] - ref.mean[i]), Math.abs(lc.curve.sd[i] - ref.sd[i]));
  }
  days += ref.mean.length;
  worst = Math.max(worst, gap);
  console.log(`  loan ${String(ref.loan).padStart(5)}  ${String(lc.marks.length).padStart(3)} marks  ${String(ref.mean.length).padStart(4)} days  largest gap ${gap.toExponential(2)}`);
}
console.log(`\n${reference.length} loans, ${days} loan-days, largest gap ${worst.toExponential(2)} of par (tolerance ${TOLERANCE.toExponential(0)})`);
if (!(worst <= TOLERANCE)) {
  console.error("the browser and the native program disagree");
  process.exit(1);
}
