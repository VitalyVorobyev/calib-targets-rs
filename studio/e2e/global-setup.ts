/**
 * Start the studio's Rust server in API-only mode for the screenshot suite.
 *
 * Built with `cargo build --release -p calib-targets-studio` first. It serves the bench
 * dataset manifest on :8930, where Vite's dev server proxies `/api` (vite.config.ts).
 */

import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const REPO = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const SERVER = join(REPO, "target/release/calib-targets-studio");
const API = "http://127.0.0.1:8930";

async function up(): Promise<boolean> {
  try {
    return (await fetch(`${API}/api/dataset`)).ok;
  } catch {
    return false;
  }
}

export default async function globalSetup(): Promise<() => void> {
  if (!existsSync(SERVER)) throw new Error(`build the server first: cargo build --release -p calib-targets-studio`);
  if (await up()) throw new Error(`something already answers on ${API}; stop it first`);
  const server = spawn(SERVER, ["--dev"], { cwd: REPO, stdio: "ignore" });
  for (let i = 0; i < 120 && !(await up()); i++) await new Promise((r) => setTimeout(r, 250));
  if (!(await up())) {
    server.kill();
    throw new Error(`the studio server did not answer on ${API}`);
  }
  return () => {
    server.kill();
  };
}
