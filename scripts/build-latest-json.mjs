#!/usr/bin/env node
// Rebuild the updater manifest (latest.json) deterministically from a GitHub
// Release's signed updater assets.
//
// Why this exists: tauri-action merges ITS platform into whatever latest.json
// already sits on the draft release — a read-modify-write that races when two
// build legs finish at the same time. In v0.2.0 the Linux and Apple Silicon
// legs landed within a minute of each other and the published latest.json lost
// the darwin-aarch64 entries entirely, so Apple Silicon users saw no update.
// After ALL legs have uploaded, this script throws the merged file away and
// rebuilds it from the .sig assets actually present on the release, then
// replaces the asset. Single writer, no race.
//
// Two halves, deliberately separated:
//   buildLatestJson()  — pure, unit-tested (scripts/build-latest-json.test.ts)
//   main()             — GitHub REST I/O, runs when invoked as a CLI
//
// CLI env: GITHUB_REPOSITORY, RELEASE_ID, GITHUB_TOKEN, VERSION.
// Requires Node 20+ (global fetch).

import { writeFileSync } from "node:fs";
import { readFile } from "node:fs/promises";
import { argv } from "node:process";
import { fileURLToPath } from "node:url";

/**
 * Map a signed updater asset name to the platform keys tauri-action would have
 * written for it. Returning several keys mirrors tauri-action's own aliasing
 * (e.g. `darwin-aarch64` and `darwin-aarch64-app` point at the same tarball);
 * existing installed clients look up one or the other, so both must stay.
 *
 * @param {string} sigName asset name ending in `.sig`
 * @returns {string[]} platform keys, empty when the asset is not an updater artifact
 */
function platformKeysFor(sigName) {
  const name = sigName.slice(0, -".sig".length);
  if (name.endsWith("_aarch64.app.tar.gz")) return ["darwin-aarch64", "darwin-aarch64-app"];
  if (name.endsWith("_x64.app.tar.gz")) return ["darwin-x86_64", "darwin-x86_64-app"];
  if (name.endsWith("_amd64.AppImage")) return ["linux-x86_64", "linux-x86_64-appimage"];
  if (name.endsWith("_amd64.deb")) return ["linux-x86_64-deb"];
  if (name.endsWith(".x86_64.rpm")) return ["linux-x86_64-rpm"];
  if (name.endsWith("_x64-setup.exe")) return ["windows-x86_64", "windows-x86_64-nsis"];
  if (name.endsWith("_x64_en-US.msi")) return ["windows-x86_64-msi"];
  return [];
}

// A manifest missing any of these would silently strand a whole platform on an
// old version — exactly the v0.2.0 failure. Fail the release instead.
const REQUIRED_PLATFORMS = [
  "darwin-aarch64",
  "darwin-x86_64",
  "linux-x86_64",
  "windows-x86_64",
];

/**
 * Build the updater manifest from the release's signature assets.
 *
 * @param {object} args
 * @param {string} args.version release version, without a leading `v`
 * @param {string} args.pubDate ISO-8601 publication timestamp
 * @param {string} args.notes release notes shown by the updater
 * @param {string} args.repo `owner/name`
 * @param {{ name: string, sigText: string }[]} args.assets `.sig` assets with their contents
 * @returns {{ version: string, notes: string, pub_date: string, platforms: Record<string, { signature: string, url: string }> }}
 */
export function buildLatestJson({ version, pubDate, notes, repo, assets }) {
  /** @type {Record<string, { signature: string, url: string }>} */
  const platforms = {};

  for (const { name, sigText } of assets) {
    if (!name.endsWith(".sig")) continue;
    const keys = platformKeysFor(name);
    if (keys.length === 0) continue;
    const entry = {
      signature: sigText.trim(),
      // The `releases/latest/download/<name>` shape is what tauri-action emits
      // today; installed clients already fetch it, so keep it byte-identical.
      url: `https://github.com/${repo}/releases/latest/download/${name.slice(0, -".sig".length)}`,
    };
    for (const key of keys) platforms[key] = entry;
  }

  const missing = REQUIRED_PLATFORMS.filter((p) => !(p in platforms));
  if (missing.length > 0) {
    throw new Error(
      `latest.json is missing required platform(s): ${missing.join(", ")}. ` +
        `Signed assets seen: ${assets.map((a) => a.name).join(", ") || "(none)"}`,
    );
  }

  // Sort the keys so the manifest is byte-stable across runs and diffable.
  /** @type {Record<string, { signature: string, url: string }>} */
  const sorted = {};
  for (const key of Object.keys(platforms).sort()) sorted[key] = platforms[key];

  return { version, notes, pub_date: pubDate, platforms: sorted };
}

// ---------------------------------------------------------------------------
// CLI: GitHub REST I/O
// ---------------------------------------------------------------------------

const API = "https://api.github.com";

/** @param {string} url @param {RequestInit} [init] */
async function gh(url, init = {}) {
  const res = await fetch(url, {
    ...init,
    headers: {
      Authorization: `Bearer ${process.env.GITHUB_TOKEN}`,
      Accept: "application/vnd.github+json",
      "X-GitHub-Api-Version": "2022-11-28",
      "User-Agent": "ken-release",
      ...(init.headers ?? {}),
    },
  });
  if (!res.ok) {
    throw new Error(`${init.method ?? "GET"} ${url} -> ${res.status} ${await res.text()}`);
  }
  return res;
}

/** List every asset on the release, following pagination. */
async function listAssets(repo, releaseId) {
  const all = [];
  for (let page = 1; ; page++) {
    const res = await gh(
      `${API}/repos/${repo}/releases/${releaseId}/assets?per_page=100&page=${page}`,
    );
    const batch = await res.json();
    all.push(...batch);
    if (batch.length < 100) return all;
  }
}

async function main() {
  const repo = process.env.GITHUB_REPOSITORY;
  const releaseId = process.env.RELEASE_ID;
  const version = (process.env.VERSION ?? "").replace(/^v/, "");
  if (!repo || !releaseId || !version || !process.env.GITHUB_TOKEN) {
    throw new Error(
      "GITHUB_REPOSITORY, RELEASE_ID, VERSION and GITHUB_TOKEN must all be set.",
    );
  }

  const assets = await listAssets(repo, releaseId);
  const sigAssets = assets.filter((a) => a.name.endsWith(".sig"));
  console.log(`Release ${releaseId}: ${assets.length} assets, ${sigAssets.length} signatures.`);

  const withText = [];
  for (const asset of sigAssets) {
    // Draft-release assets are not downloadable from browser_download_url yet;
    // the API asset URL with Accept: octet-stream serves the bytes.
    const res = await gh(asset.url, { headers: { Accept: "application/octet-stream" } });
    withText.push({ name: asset.name, sigText: await res.text() });
  }

  let notes = "";
  try {
    notes = await readFile("RELEASE_NOTES.md", "utf8");
  } catch {
    /* notes are optional */
  }

  const manifest = buildLatestJson({
    version,
    pubDate: new Date().toISOString(),
    notes,
    repo,
    assets: withText,
  });
  const json = `${JSON.stringify(manifest, null, 2)}\n`;
  writeFileSync("latest.json", json);
  console.log(`Rebuilt latest.json with platforms: ${Object.keys(manifest.platforms).join(", ")}`);

  // Replace the racily-merged asset tauri-action left behind.
  for (const existing of assets.filter((a) => a.name === "latest.json")) {
    await gh(`${API}/repos/${repo}/releases/assets/${existing.id}`, { method: "DELETE" });
    console.log(`Deleted stale latest.json asset ${existing.id}.`);
  }
  await gh(
    `https://uploads.github.com/repos/${repo}/releases/${releaseId}/assets?name=latest.json`,
    { method: "POST", headers: { "Content-Type": "application/json" }, body: json },
  );
  console.log("Uploaded latest.json.");
}

// Only run the I/O half when executed directly, so tests can import the module.
if (argv[1] && fileURLToPath(import.meta.url) === argv[1]) {
  main().catch((err) => {
    console.error(`::error::${err.message}`);
    process.exit(1);
  });
}
