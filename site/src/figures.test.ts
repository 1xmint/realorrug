// SPDX-License-Identifier: Apache-2.0
//! Every figure this site still publishes, against the file it came from.
//!
//! # Most of this file is gone, and that is the point
//!
//! Until the Robinhood Chain move, this file pinned `index.html` and
//! `Home.tsx` to `fixtures/stats.json`, which was itself pinned to
//! `docs/research/data/0024-base-rates.json` — the Solana/pump.fun
//! launch-block figures. Those figures came off the home page (see
//! `Home.tsx`'s module comment) and the fixture they were pinned to is
//! deleted with them, so the tests that checked them are deleted too rather
//! than left pinning a file that no longer exists. What is left is the
//! figures the site still publishes: the fee ladder and the share-card image
//! dimensions, plus the verdict-language check, which is not a figure at all
//! but earned its place in this file for the same reason.
//!
//! - `index.html` no longer carries any measured figure — see it directly.
//! - The fee ladder is **not** checked against the chain here. Its capture is
//!   hex, and decoding it in TypeScript would be a second implementation of
//!   `radar-pumpfun`'s fee parser. That check lives in that crate, in
//!   `the_site_publishes_the_ladder_this_crate_decodes.rs`, where the decoder
//!   already is. Named here so the absence reads as a decision.

import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { describe, expect, it } from "vitest";

import ladder from "./fixtures/fee-ladder.json";

/**
 * A file from the repository, by path relative to `site/`.
 *
 * Resolved from the vitest root rather than from `import.meta.url`: under
 * vitest that URL is rewritten to a `/@fs/` form, and `fileURLToPath` turns it
 * into a path with the drive letter twice. `process.cwd()` is the site
 * directory for every runner configured here.
 */
function repoFile(relative: string): string {
  return readFileSync(resolve(process.cwd(), relative), "utf8");
}

const INDEX_HTML = repoFile("index.html");
const HOME = repoFile("src/Home.tsx");

describe("the card that unfurls when the link is shared", () => {
  const png = readFileSync(resolve(process.cwd(), "public/og.png"));

  it("is a PNG at the size the meta tags promise", () => {
    // Every unfurler crops to what og:image:width and og:image:height say. A
    // file whose real size is not that size is letterboxed by somebody else's
    // rules, on the one image that is the product's first impression.
    expect(png.subarray(0, 8).toString("hex")).toBe("89504e470d0a1a0a");
    // IHDR is the first chunk: 8 bytes of magic, 4 of length, 4 of type, then
    // width and height as big-endian u32.
    expect(png.readUInt32BE(16)).toBe(1200);
    expect(png.readUInt32BE(20)).toBe(630);
    expect(INDEX_HTML).toContain('content="1200"');
    expect(INDEX_HTML).toContain('content="630"');
  });
});

describe("the fee ladder the tokenomics page renders", () => {
  // Robinhood Chain's Pons v2 curve is a flat rate, not pump.fun's
  // market-cap-keyed table, and there is no post-graduation number to check —
  // research 0040 found the graduated pool's fee split was never decoded.
  // What is checked here is that the fixture keeps the shape the page reads,
  // so a truncated fixture fails as a test rather than as an empty page.
  it("has the curve's flat fee, and admits graduation is not established", () => {
    expect(ladder.curve.base_fee_bps).toBe(100);
    expect(ladder.curve.creator_base_share_bps).toBe(7000);
    expect(ladder.curve.protocol_share_bps).toBe(3000);
    expect(ladder.curve.creator_tax_ceiling_bps).toBe(1000);
    expect(ladder.after_graduation.established).toBe(false);
  });
});

describe("the page states measurements rather than verdicts", () => {
  // Finding H4. The hero said "Most launches are coordinated" while the card
  // directly under it measured the opposite. That sentence and the card it
  // contradicted are both gone with the Solana figures, but the rule it
  // established outlives them: no page states a verdict about launches in
  // general, on Solana's numbers or on any other chain's.
  //
  // **Comments are stripped before the check**, and the first version of this
  // test was not — it failed on the comment that explains why the sentence was
  // removed, which is the one place the old wording legitimately survives. A
  // check on published copy has to read published copy.
  /** Source with its comments removed: HTML, JSX and line comments. */
  function copyOnly(text: string): string {
    return text
      .replace(/<!--[\s\S]*?-->/g, " ")
      .replace(/\{\/\*[\s\S]*?\*\/\}/g, " ")
      .replace(/^\s*\/\/.*$/gm, " ")
      .toLowerCase();
  }

  it.each([
    ["index.html", INDEX_HTML],
    ["Home.tsx", HOME],
  ])("%s makes no claim about most launches", (_name, text) => {
    const copy = copyOnly(text);
    expect(copy).not.toContain("most launches are coordinated");
    expect(copy).not.toContain("most launches on pump.fun are coordinated");
  });

  it("the comment stripper actually removes a comment", () => {
    // Otherwise the test above passes because it reads nothing. The stripper is
    // the only moving part in it.
    expect(copyOnly("<!-- most launches are coordinated -->")).not.toContain(
      "coordinated",
    );
    expect(copyOnly("{/* most launches are coordinated */}")).not.toContain(
      "coordinated",
    );
    expect(copyOnly("most launches are coordinated")).toContain("coordinated");
  });
});
