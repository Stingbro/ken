---
name: release
description: "Cut a Ken release: summarize changes into WHATS_NEW.md, bump the version, sync manifests, run tests, commit and push so CI tags and builds installers"
---

# Cutting a Ken release

A release is a version bump on `main`. CI does the rest: `.github/workflows/release.yml`
sees the bump, runs `scripts/release-gate.sh`, syncs the manifests, regenerates
`CHANGELOG.md`, commits `chore(release): v<version> [skip ci]`, tags `v<version>`,
builds the four-target installer matrix, and publishes the GitHub Release once
every leg has uploaded.

Your job is everything before the push — above all the `WHATS_NEW.md` entry,
which is what users actually read in the in-app "What's new" dialog.

## 1. Preconditions

```sh
git rev-parse --abbrev-ref HEAD    # must be main
git status --porcelain             # must be empty
git pull --ff-only
```

Stop and tell the user if the branch isn't `main` or the tree isn't clean. Never
stash or commit unrelated work to get there.

## 2. Read the changes

```sh
last="$(git describe --tags --abbrev=0)"
git log "$last"..HEAD --oneline --no-merges
```

If that list is empty there is nothing to release — say so and stop. Read the
diffs of anything whose user-facing effect isn't obvious from the subject.

## 3. Choose the version

`package.json`'s `version` is the source of truth. From the current version:

- **patch** (`0.1.2` → `0.1.3`) — fixes and small improvements to existing
  features. The default.
- **minor** (`0.1.2` → `0.2.0`) — a new capability users will notice as new:
  a new screen, a new document type, a new workflow.

State the choice and why in one line before editing anything.

## 4. Write the WHATS_NEW.md entry

Add it at the top, directly under `# What's New` (newest first):

```md
## <version> — <YYYY-MM-DD>

### <Section>

- **Key part**: what changed, in the user's words.
```

Rules (the file's header comment says the same):

- Only user-facing changes. No refactors, no test/CI plumbing, no dependency
  bumps, no commit-message noise, no file paths.
- Group into one or more `###` sections named for where the change shows up
  ("Editor", "Search", "Ingests", "Releases").
- Every item is `- **Key part**: short description.` — the bold key is the
  feature's name, the body is one sentence a user understands.
- Several commits about one feature collapse into one item.
- Inline markdown is limited to code spans and links.

## 5. Bump, sync and verify

```sh
# edit package.json's "version" to <version>, then:
scripts/sync-version.sh <version>      # tauri.conf.json, Cargo.toml, Cargo.lock
scripts/check-whats-new.sh <version>   # fails if the entry is missing
pnpm test && pnpm check
```

All four must pass. Fix real failures; never weaken a test to get green.

## 6. Commit and push

```sh
git add WHATS_NEW.md package.json src-tauri/tauri.conf.json Cargo.toml Cargo.lock
git commit -m "chore(release): bump version to <version>"
git push origin main
```

Do not tag — CI creates the tag. Do not add `[skip ci]`; that would suppress
the whole release.

## 7. Confirm

```sh
gh run watch                                  # or: gh run list --workflow=Release
gh release view "v<version>"                  # draft → published when all 4 legs upload
```

The release is done when `v<version>` exists as a **published** (non-draft)
release carrying macOS (both arches), Linux and Windows installers plus the
`ken-mcp` binaries and `latest.json`.

## Recovering

- **Gate fails on the What's New check** — the push landed but no tag was
  created. Add the missing `## <version>` entry to `WHATS_NEW.md`, commit, and
  push again; the gate re-runs and releases.
- **A build leg fails** — the tag and the draft Release already exist, and
  `release-gate.sh`'s idempotency guard refuses to re-release that version. Fix
  the build, then cut the next patch version (a fresh bump). Delete the stuck
  draft Release first.
- **Testing packaging without burning a version** — dispatch the Release
  workflow with `bundle_test: true`; it builds the Windows installer as a
  workflow artifact and never tags or releases.
