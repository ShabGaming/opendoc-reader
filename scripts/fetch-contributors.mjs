#!/usr/bin/env node
// Bakes the GitHub contributor list into the app at build time, so the running app never
// makes a network call for it.
//
//   node scripts/fetch-contributors.mjs              fetch now
//   node scripts/fetch-contributors.mjs --ci-only    fetch only when CI=true (release builds)
//
// Writes apps/desktop/src/assets/contributors/contributors.json and one 64 px avatar per
// person next to it. Uses GITHUB_TOKEN when set. On any failure it keeps the files that are
// already committed, prints a warning and exits 0, so offline and development builds work.

import { mkdir, readFile, rename, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const REPO = "ShabGaming/opendoc-reader";
const MAINTAINER = "ShabGaming";
const here = path.dirname(fileURLToPath(import.meta.url));
const outDir = path.join(here, "..", "apps", "desktop", "src", "assets", "contributors");
const listFile = path.join(outDir, "contributors.json");

function warn(message) {
  console.warn(`fetch-contributors: ${message}`);
}

function headers(accept) {
  const h = { accept, "user-agent": "opendoc-reader-build" };
  if (process.env.GITHUB_TOKEN) h.authorization = `Bearer ${process.env.GITHUB_TOKEN}`;
  return h;
}

async function getJson(url) {
  const response = await fetch(url, { headers: headers("application/vnd.github+json") });
  if (!response.ok) {
    const error = new Error(`${url} answered ${response.status}`);
    error.status = response.status;
    throw error;
  }
  return response.json();
}

async function getAvatar(login, avatarUrl) {
  const url = new URL(avatarUrl);
  url.searchParams.set("s", "64");
  const response = await fetch(url, { headers: headers("image/*") });
  if (!response.ok) throw new Error(`avatar of ${login} answered ${response.status}`);
  return Buffer.from(await response.arrayBuffer());
}

function isBot(user) {
  return user.type === "Bot" || user.login.endsWith("[bot]");
}

async function listContributors() {
  const people = [];
  for (let page = 1; page <= 10; page += 1) {
    const batch = await getJson(
      `https://api.github.com/repos/${REPO}/contributors?per_page=100&page=${page}`,
    );
    people.push(...batch);
    if (batch.length < 100) break;
  }
  return people.filter((u) => !isBot(u));
}

async function displayName(login) {
  try {
    const user = await getJson(`https://api.github.com/users/${encodeURIComponent(login)}`);
    return user.name || null;
  } catch {
    return null;
  }
}

/** The maintainer alone, for when the repository is not public yet. */
async function fallback() {
  const avatar = await getAvatar(MAINTAINER, `https://github.com/${MAINTAINER}.png?size=64`);
  return [
    {
      login: MAINTAINER,
      name: null,
      profileUrl: `https://github.com/${MAINTAINER}`,
      contributions: 1,
      avatar: `${MAINTAINER}.png`,
      bytes: avatar,
    },
  ];
}

async function collect() {
  let users;
  try {
    users = await listContributors();
  } catch (error) {
    if (error.status !== 404) throw error;
    warn("the repository is not visible yet (404); using the maintainer only");
    return fallback();
  }
  const entries = [];
  for (const user of users) {
    entries.push({
      login: user.login,
      name: await displayName(user.login),
      profileUrl: user.html_url,
      contributions: user.contributions,
      avatar: `${user.login}.png`,
      bytes: await getAvatar(user.login, user.avatar_url),
    });
  }
  if (entries.length === 0) throw new Error("the API returned no contributors");
  return entries;
}

async function main() {
  if (process.argv.includes("--ci-only") && process.env.CI !== "true") {
    console.log("fetch-contributors: not a CI build, keeping the committed list");
    return;
  }
  const entries = await collect();
  entries.sort((a, b) => b.contributions - a.contributions || a.login.localeCompare(b.login));
  await mkdir(outDir, { recursive: true });
  for (const entry of entries) {
    // Write beside the target first so a failed run never leaves a half-written avatar.
    const target = path.join(outDir, entry.avatar);
    await writeFile(`${target}.tmp`, entry.bytes);
    await rename(`${target}.tmp`, target);
  }
  const list = entries.map(({ bytes, ...rest }) => rest);
  const next = `${JSON.stringify(list, null, 2)}\n`;
  const before = await readFile(listFile, "utf8").catch(() => "");
  if (before !== next) await writeFile(listFile, next);
  console.log(`fetch-contributors: ${entries.length} contributor(s)`);
}

main().catch((error) => {
  warn(`${error.message}; keeping the committed list`);
});
