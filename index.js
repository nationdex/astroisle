import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

export function findAstroisleBin() {
  const candidates = [
    join(import.meta.dir, "target/release/astroisle"),
    join(process.env.HOME || "", ".cargo/bin/astroisle"),
    "/usr/local/bin/astroisle",
    "astroisle",
  ];
  return candidates.find((p) => p === "astroisle" || existsSync(p)) || "astroisle";
}

export function runAstroisle(options = {}) {
  const bin = options.bin || findAstroisleBin();
  const args = ["run"];

  if (options.dist) args.push("--dist", options.dist);
  if (options.budgetInitial) args.push("--budget-initial", String(options.budgetInitial));
  if (options.budgetLazy) args.push("--budget-lazy", String(options.budgetLazy));
  if (options.budgetCss) args.push("--budget-css", String(options.budgetCss));
  if (options.nstdBin) args.push("--nstd-bin", options.nstdBin);
  if (options.level) args.push("--level", String(options.level));
  if (options.noSw) args.push("--no-sw");

  const res = spawnSync(bin, args, { stdio: "inherit", env: process.env });
  if (res.status !== 0) {
    throw new Error(`astroisle exited with code ${res.status}`);
  }
}

/**
 * Astro Integration: automatically executes astroisle post-build asset pipeline.
 */
export default function astroisle(options = {}) {
  return {
    name: "astroisle",
    hooks: {
      "astro:build:done": async ({ dir }) => {
        const dist = typeof dir === "string" ? dir : fileURLToPath(dir);
        runAstroisle({ dist, ...options });
      },
    },
  };
}
