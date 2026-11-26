# The Crisp ingredient index

This document records what the Crisp ingredient index is, where it lives, and
how it is maintained. It is a decision record: each section states what the
index must do, and the follow-up issues listed at the end implement the parts
the code does not do yet.

The index gives ingredients a permanent, language-neutral identity. A recipe
still names each ingredient in free text, and may also carry the slug of the
index entry it is (`ingredient.code`, spec §4). With that slug, a recipe
written in Korean can be found by a search in English, a shopping list can
group what it lists, and an editor can recognize "묵은지" as well-aged kimchi.

It covers the dataset, its sources, and its governance. Server routes for
suggestions and search (#97, #98) and the editor UI (#99) are described here
only where they depend on its decisions.

## Current state

At the head of the pull request that introduced the index:

- `spec/ingredients/` holds the published dataset: 3,646 entries, names in 16
  locale shards, 471 densities, and allergen/diet hints on 1,334 entries.
- `crisp-core` embeds it at build time (`core/build.rs`), and
  `crisp_core::ingredients::index()` gives the server and, through
  `crisp-wasm`, the client the same index.
- `tools/ingredients` regenerates it from pinned sources and committed rules;
  CI fails when the committed dataset is stale.
- The server indexes codes for search and reports the index version in
  `GET /server-info` (#97): `GET /search?ingredient=kimchi` also finds
  recipes coded with its descendants and merged slugs, and the free-text
  match covers the index's names for coded ingredients in every language a
  bundle declares. Accounts submit suggestions to
  `POST /ingredients/suggestions` (#98).
- `crisp-wasm` exposes `matchIngredient`, `ingredientName`, and
  `ingredientEntry` (#99). The editor links typed ingredient and substitute
  names to codes and suggests entries (§3.3), and the recipe view shows the
  index's name in the reader's language beside a coded ingredient from a
  recipe in another language. The editor cannot submit a suggestion yet
  (#128).

## 1. Scope and non-goals

- **One index, many hosts.** The repository is the source of truth. Every
  `crisp-server` and every client embeds the release they were built from.
  The public instance is where the index grows, because that is where most
  suggestions arrive, but it holds no authority a self-hosted server lacks.
- **No federation.** Servers do not sync indexes with each other or with the
  public instance. A self-hosted server gets new entries by upgrading. This
  keeps every host's index reviewable and reproducible, and means no server
  can push data into another.
- **Recipes never depend on it.** The vocabulary is open (spec §13): any
  well-formed slug is valid in a bundle, a reader that does not know a code
  carries it verbatim, and a code never changes how `name` renders.
  Verification checks the slug's syntax only (B1). A bundle written against a
  newer index opens everywhere.
- **Not nutrition data.** Densities and allergen/diet hints are aids for
  conversion and suggestions. The recipe's own `allergens`, `diet`, and
  `alt` measures stay authoritative.

## 2. Data model

`crisp_core::ingredients::Dataset` is the contract between the importer and
every reader. On disk it is split so a change reviews as a small diff:

| File | Holds |
|---|---|
| `entries.json` | One entry per line: `slug`, `parent`, `category`, `density` (g/mL as a rational), `allergens`, `diet_conflicts`, `replaced_by`, `source` |
| `locales.json` | The shards, the fallback table, and the last-resort shards |
| `names/<shard>.json` | One line per entry: its display `name` and `aliases` in that shard |
| `VERSION` | SHA-256 of the files above, the dataset's identity |
| `LICENSE` | ODbL-1.0 for the data (§5) |

### 2.1 Hierarchy and specificity

Entries form a tree through `parent`. A variant that a cook must buy or
prepare differently is its own entry under the general one, and a recipe
references **the most specific entry it means**:

```text
white-cabbage
└── kimchi                 김치 · キムチ · 韩国泡菜
    └── kimchi-aged        묵은지 · 신김치 · aged kimchi · sour kimchi
```

A kimchi jjigae recipe says `code: kimchi-aged`. Search and substitution
use the hierarchy, so a search for `kimchi` also finds it
(`IngredientIndex::descendants`, `is_a`). Anything the index does not express —
"three months old", "from the bottom of the jar" — stays in the
ingredient's `preparation` or `note` text.

A qualifier that changes what you buy (aged kimchi, smoked paprika, brown
butter as a purchased product) is a child entry. A qualifier that describes
how you prepare it (chopped, softened, at room temperature) is
`preparation`, never an entry.

### 2.2 Slugs are permanent

- A slug is lowercase ASCII words joined by single hyphens (spec `slug`),
  derived once from the source's English canonical id (`en:fish-sauce` →
  `fish-sauce`). The English display name may differ by region; the slug does
  not.
- A published slug is never renamed or deleted. A re-import maps each source
  entry back to its published slug through `source`, and a published entry
  the sources stop producing is kept unchanged, names included (the report
  lists these). Every published slug stays reserved, so a new entry never
  takes one.
- Two entries found to be the same thing are merged by recording
  `"slug": "successor"` in `tools/ingredients/rules/overrides/merges.json`,
  which sets `replaced_by`. Readers resolve through it
  (`IngredientIndex::resolve`), and the hierarchy follows it: the merged
  entry and its children count under the successor (`is_a`, `descendants`),
  so a search expanded from the successor still finds bundles coded with the
  old slug. A merge, once published, is permanent like the slug: the
  importer refuses to drop its line from `merges.json`.
- A collision between new slugs takes the lowest free `-2`, `-3`, … suffix,
  assigned in source-key order.

### 2.3 Categories, density, hints

- `category` is a coarse grouping (`produce`, `dairy`, `spice`, …) for
  browsing and shopping lists, taken from the nearest included source subtree
  above the entry through any of its source parents (§4).
- `density` relates volume and mass. `convert_with_density` gives an
  approximate measure. The recipe view offers it as `derived` (`"≈ 140 g"`)
  only for an ingredient whose author gave no `alt`, so an author's measure
  always wins; the web client shows it only when the reader turns on
  Settings → Reading → "Show derived measures" (off by default), in a
  secondary style. The editor's "Fill from density" writes it into `alt`,
  where the author owns and may edit the number.
- `allergens` and `diet_conflicts` are hints an editor may use to *suggest* a
  recipe's own fields. They come from source properties, inherited down the
  source hierarchy, and from fixed allergen → diet rules (milk → not
  `dairy-free`).

## 3. Locales

### 3.1 Region-qualified shards

Names are sharded by region, never by bare language: British and American
English, and Mainland, Taiwanese, and Hong Kong Chinese, name the same
ingredient differently. The shards are:

| Language | Shards |
|---|---|
| English | `en-US`, `en-GB`, `en-CA` |
| Chinese | `zh-Hans-CN`, `zh-Hant-TW`, `zh-Hant-HK` (Macau falls back to Hong Kong) |
| East and Southeast Asia | `ja-JP`, `ko-KR`, `vi-VN`, `th-TH` |
| Europe | `fr-FR`, `it-IT`, `es-ES`, `de-DE`, `pt-PT`, `el-GR` |

A shard holds a name only where it has one. Nothing is copied into a shard to
fill a gap: `en-GB` lists `aubergine` for `aubergine`, and holds nothing for
`kimchi`, which it reads from `en-US`.

Adding a shard is a rules change (§4) plus, usually, a new source language
mapping or override file; it changes no slug.

### 3.2 Fallback

A recipe's text may be tagged broadly (`en`, `zh-Hant`) or with a region
that has no shard (`en-AU`). Every lookup walks a chain, best first:

1. the shard that is exactly the tag;
2. the tag's entry in the fallback table, then each truncation's
   (`zh-Hant-MO` → `zh-Hant` → `zh`);
3. the language's other shards, in declared order;
4. the last-resort shards (English).

So `en-AU` reads `en-GB` then `en-US`; `zh-Hant-MO` reads `zh-Hant-HK`, then
`zh-Hant-TW`, then `zh-Hans-CN`. The table is in `rules.toml` and published
in `locales.json`.

### 3.3 Editor behaviour

`IngredientIndex::match_name(text, tag, limit)` is the contract the editor
(#99) builds on:

- Typed text and every index spelling are folded the same way for
  *finding*: lowercase, compatibility-normalized, Latin and Greek accents
  dropped, punctuation as spaces. Thai, Japanese, and Korean marks are kept.
- **Exact link.** In the first shard of the chain that has the folded text at
  all, the text equals — accents included — the spelling of exactly one
  entry. The editor attaches that slug at once and shows it as a removable
  chip. Accents decide meaning in some languages (`bò` is beef, `bơ` is
  butter; `pâté` is not `pâte`), so a match only up to accents is a
  suggestion, never a link.
- **Suggestions.** Otherwise — a spelling two entries share, a prefix, a
  word — the result is ranked candidates. A hit in exactly the asked-for
  shard ranks above one found by fallback. The author picks one or none.
- The author's `name` is never rewritten, and only index slugs are attached.
  A code in a received bundle that this index does not know still renders.

## 4. How the index is built

`just ingredients` fetches the pinned sources into `target/ingredients-cache/`
and runs `tools/ingredients`, which:

1. verifies each source's SHA-256 against its pin, and stops on a mismatch;
2. parses the Open Food Facts ingredients taxonomy;
3. **selects** entries by class rules: included subtrees, each mapped to a
   category; excluded subtrees (flavourings, extracts, coatings, …);
   excluded name patterns (additive codes, percentages, origin and
   certification labels, preparations); an English canonical id is required;
4. assigns slugs (§2.2) and parents (the nearest selected ancestor);
5. fills shards: each source language → its default shard; English through
   the regional lexicon (`en-regional.toml`, US/GB/CA phrase rewriting); OFF's
   mixed-script Chinese normalized to Simplified, then `zh-Hant-TW` and
   `zh-Hant-HK` derived with OpenCC's regional tables;
6. derives densities (§5) and hints;
7. applies overrides;
8. validates the result as an `IngredientIndex`, writes `spec/ingredients/`,
   and writes `tools/ingredients/report/` — the count of entries each rule
   included or excluded, per-shard coverage, density coverage, retained
   entries, and every USDA candidate with its kind and acceptance.

The output is a pure function of the pinned sources, the rules, the
overrides, and the previously published entries and names. `just
ingredients-check` (run in CI) regenerates it in memory and fails on any
difference, including a leftover shard file `core/build.rs` would embed.
CI caches the snapshots by the hash of `rules.toml`, so the source hosts are
contacted only when a pin changes; locally, a cached file that no longer
matches its pin is fetched again.

### 4.1 Rules, not entries

Decisions are made about classes, never one entry at a time:

| To change | Edit |
|---|---|
| Which part of the taxonomy is imported, and its category | `[select] include` / `exclude_subtrees` in `rules.toml` |
| A family of label noise to drop | a `[[select.exclude]]` pattern |
| A US/UK/Canadian term | a `[[term]]` in `en-regional.toml` (a rewrite never takes a spelling another entry owns, so the lexicon renames but never merges) |
| Two entries that are the same ingredient | a line in `overrides/merges.json` |
| Which densities are trusted | `[density] accept` |
| The locale shards or fallback order | `[locales]` |
| A source snapshot | the `url` and `sha256` in `[sources]` |

The report shows what a rule change did, by count, so a reviewer judges the
rule rather than thousands of rows.

The only per-entry channel is `tools/ingredients/rules/overrides/`:
`entries.json` adds entries the sources lack (`kimchi-aged`),
`names/<shard>.json` sets a display name or adds aliases (retained entries
included), and `merges.json` records merges. An override that names an
unknown entry or an undeclared shard stops the import rather than being
ignored. Overrides are where
accepted suggestions land (§6).

## 5. Sources and licensing

| Source | Used for | Licence |
|---|---|---|
| [Open Food Facts](https://world.openfoodfacts.org) ingredients taxonomy, pinned to an `openfoodfacts-server` commit | entries, hierarchy, names in many languages, allergen/vegan/vegetarian properties, a few densities, links to USDA foods | ODbL-1.0 |
| USDA FoodData Central, SR Legacy (2018-04) | densities from portion weights (cup, tbsp, tsp, fl oz) | Public domain (CC0) |
| [OpenCC](https://github.com/BYVoid/OpenCC) dictionaries, pinned to a commit | Simplified normalization; Taiwan and Hong Kong conversion | Apache-2.0 |

Density, best source first: the taxonomy's own `density_g_per_ml`; a USDA
food the taxonomy links (`linked`); a USDA food whose one-segment description
equals an English name (`exact`). Several foods of the best kind give their
median; the value is rounded to hundredths. Matches by shared words (`fuzzy`) are
recorded in `usda-map.json` but not accepted. Each USDA food's density is the
median over its plain volume portions (`1 cup`), or over all of them when it
has none plain.

Because Open Food Facts is ODbL, the dataset in `spec/ingredients/` is ODbL
too (see its `LICENSE`), separately from the AGPL code. That costs little:
the index is public anyway, and a recipe that references a slug is a Produced
Work, not an adapted database. A host that redistributes an *altered* index
must publish it under the ODbL.

## 6. Suggestions and releases

The index grows through suggestions, and every accepted suggestion becomes a
reviewed change to this repository:

1. **Suggest.** A signed-in user on any server proposes a missing entry, a
   name or alias in a locale, a regional spelling, or a merge
   (`POST /ingredients/suggestions`, #98).
2. **Triage.** That server's administrators accept, decline, or mark
   duplicates in an admin queue; each action is audited like other moderation
   actions (`docs/public-server-policy.md` §4.5).
3. **Upstream.** Accepted suggestions export as override (or rule) patches.
   The public instance's administrators open them as pull requests here; a
   self-hoster may do the same, or open an issue.
4. **Review.** A maintainer merges after checking the regenerated report: a
   class-level fix becomes a rule; a one-off becomes an override.
5. **Release.** The next release embeds the new dataset. Servers and the web
   client that deploy it carry the same `VERSION`; `GET /server-info` reports
   it (#97).

Between releases a server keeps answering with the index it was built with.
A recipe can use a slug before every reader's index knows it (§1), so nothing
waits on a release except names and suggestions.

Re-importing a newer source snapshot is a pull request that changes the pins.
Its report diff shows what changed; slugs already published stay put.

### Self-hosting

A self-hosted server embeds exactly the index of the release it runs, so it
upgrades the index by upgrading ([deployment.md](deployment.md#upgrades)). It
may run its own suggestion queue (#98). Local entries that never go upstream
would need a fork of the dataset, and an altered dataset that is publicly
used must stay ODbL.

## 7. Distribution

The whole index is embedded in `crisp-core`: 1.17 MB of CBOR today, and a
test fails above 2 MiB. The client module links it for density-derived
measures (#100) and for matching and names (#99): `crisp_bg.wasm` is
5.85 MB, 1.21 MB gzip, and the matcher and names add 66 KB raw and 11 KB
gzip of that. Past that point the plan is to keep slugs, hierarchy, and attributes
embedded and load locale shards on demand, cached for offline use. That
would be a change to this section, not to the data model.

## Follow-up issues

| Issue | Covers |
|---|---|
| #97 | §3.3, §6: index version in `/server-info`, an ingredient search facet with hierarchy rollup, index names in search text |
| #98 | §6: suggestion intake, admin triage, export of accepted suggestions |
| #99 | §3.3, §7: matcher and names in `crisp-wasm`, editor auto-link and suggestions |
| #100 | §2.3: whether and how viewers show density-derived measures |
