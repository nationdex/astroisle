#!/usr/bin/env bun
import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { join } from "node:path";

// Locate astroisle native binary
const candidates = [
  join(import.meta.dir, "../target/release/astroisle"),
  join(process.env.HOME || "", ".cargo/bin/astroisle"),
  "/usr/local/bin/astroisle",
  "astroisle",
];

const bin = candidates.find((p) => p === "astroisle" || existsSync(p)) || "astroisle";

const result = spawnSync(bin, process.argv.slice(2), {
  stdio: "inherit",
  env: process.env,
});

process.exit(result.status ?? 0);
