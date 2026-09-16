// SPDX-License-Identifier: Apache-2.0
//! The whole argument, on one page.
//!
//! # Robinhood Chain, not Solana — and no borrowed numbers
//!
//! This page used to lead with Solana/pump.fun base-rate measurements
//! (`fixtures/stats.json`, `docs/research/data/0024-base-rates.json`): a
//! launch-block recipient distribution, a graduation rate, a round-trip cost.
//! Every one of those figures is still true about Solana. None of them is
//! true about this product any more, which moved to Robinhood Chain (design
//! 0025 §0, ADR — see `docs/research/0038` through `0040`). A number that is
//! true about the wrong chain is not a smaller claim than a wrong number; it
//! is the same failure `honesty.ts` exists to catch, just imported from
//! somewhere else. So it comes off, in full, rather than staying up relabeled.
//!
//! What replaces it is design 0025 §4a's layout: the paste box (unchanged —
//! it already reads whatever chain the server tells it to), the product's own
//! token address (deny-by-default, see `TokenAddress.tsx`), a contest teaser
//! built from the same `leaderboard()` the `/contest` page uses, and a short
//! link to `/how-it-works` rather than a restatement of it.
//!
//! # The live feed is not here, and the gap is the point
//!
//! Design 0025 §4a item 5 calls for a short list of recently checked tokens.
//! No route returns that list today — `/v1/public/stats`, `/leaderboard`,
//! `/pool` and `/weeks` hold none of it, and `check()` in `api.ts` reads one
//! address at a time, on demand, never a history. A fake or hard-coded list
//! here would be exactly the LEARNINGS-5 failure the rest of this site works
//! to avoid: a section that looks like it ran when nothing did. So there is
//! no live-feed section below. It comes back once `realorrug-serve` ships the
//! recency-sorted read design 0025 §4a names as a requirement on that crate.

import { useEffect, useState } from "react";
import { Link } from "wouter";

import { leaderboard as fetchLeaderboard, type Leaderboard as Data } from "./api";
import { count } from "./honesty";
import { TokenAddress } from "./TokenAddress";
import { Card, Heading, Nothing, Section, CheckBox, Summoner } from "./ui";

/** The small label that numbers an act, in the same idiom the rest of the site uses. */
function Act({ n, children }: { n: string; children: React.ReactNode }) {
  return (
    <div className="mb-3 flex items-baseline gap-3">
      <span className="act-no" aria-hidden="true">
        {n}
      </span>
      <span className="font-mono text-xs tracking-widest text-[var(--color-signal-dim)] uppercase">
        {children}
      </span>
    </div>
  );
}

function Hero() {
  return (
    <Section className="pt-16 pb-8 sm:pt-24">
      <div className="enter">
        <div className="mb-5">
          <Act n="01">Robinhood Chain · checked, not predicted</Act>
        </div>
        {/* No "measured since <date>" here. The wireframe (docs/design/0025
            §"ASCII wireframes") shows one, but nothing in docs/research/0038,
            0039 or 0040 establishes a date this product started measuring
            Robinhood Chain -- those documents date the *chain's* factory
            deployment (2026-08-03) and this session's own read times, not a
            start date for Real or Rug's coverage of it. Printing one anyway
            would be exactly the invented figure `honesty.ts` exists to
            refuse (AGENTS.md rule 8: absent is not zero). */}
        <h1 className="display max-w-3xl text-[length:var(--text-display)] leading-[1.03] font-semibold text-balance">
          Check a token{" "}
          <span className="text-[var(--color-signal)]">before you buy.</span>
        </h1>
        <p className="mt-6 max-w-2xl text-[length:var(--text-lead)] text-[var(--color-dim)]">
          Paste a Robinhood Chain contract address and Real or Rug reads the
          chain right then — the launch, the creator&apos;s history, what
          actually happened — and hands back what it found. Not a prediction,
          not advice: what the chain shows.
        </p>
        <CheckBox />
        <TokenAddress />
      </div>
    </Section>
  );
}

/**
 * This week's top three, or the honest sentence that no week has closed.
 *
 * Worded to match `Leaderboard.tsx`'s own empty state on purpose — a reader
 * who follows "See full contest →" should not land on a page that contradicts
 * what this one just told them.
 */
function ContestTeaser() {
  const [data, setData] = useState<Data | null>(null);

  useEffect(() => {
    let live = true;
    void fetchLeaderboard().then((next) => {
      if (live) setData(next);
    });
    return () => {
      live = false;
    };
  }, []);

  const top3 = data?.entries.slice(0, 3) ?? [];

  return (
    <Section id="contest">
      <Act n="02">The contest</Act>
      <Heading>This week&apos;s top three</Heading>
      {data === null ? null : top3.length === 0 ? (
        <Nothing
          what="No week has run yet."
          why="The account is live and answering, and no week has closed yet. When one does, the best question of the week wins the whole prize pool."
        />
      ) : (
        <Card className="max-w-2xl">
          <ol className="space-y-3">
            {top3.map((e) => (
              <li
                key={`${e.rank}-${e.summoner}`}
                className="flex items-center justify-between gap-4 text-sm"
              >
                <span className="flex items-center gap-3">
                  <span className="tnum text-[var(--color-faint)]">
                    {e.rank}
                  </span>
                  <Summoner id={e.summoner} handle={e.handle} />
                </span>
                <span className="tnum text-[var(--color-dim)]">
                  {e.score === null ? "—" : `${count(e.score)} pts`}
                </span>
              </li>
            ))}
          </ol>
        </Card>
      )}
      <p className="mt-6">
        <Link
          href="/contest"
          className="text-[var(--color-signal)] underline underline-offset-4 hover:text-[var(--color-text)]"
        >
          See full contest →
        </Link>
      </p>
    </Section>
  );
}

/** What it will never do, in one paragraph, with the page that says the rest. */
function NeverSays() {
  return (
    <Section id="never">
      <Act n="03">What it will never say</Act>
      <Heading>The refusals are the product</Heading>
      <p className="max-w-2xl text-[var(--color-dim)]">
        No price target, no "buy" or "sell", no "this one is safe". Real or
        Rug reports what the chain shows and lets the numbers say it —
        enforced in code, on every reply, before it is sent.
      </p>
      <p className="mt-4">
        <Link
          href="/how-it-works"
          className="text-[var(--color-signal)] underline underline-offset-4 hover:text-[var(--color-text)]"
        >
          How it decides, and what it will never do →
        </Link>
      </p>
    </Section>
  );
}

export function Home() {
  return (
    <>
      <Hero />
      {/* Live feed of latest verdicts: design 0025 §4a item 5. No route
          exists yet to read "recently checked tokens" -- see the module
          comment above. Nothing renders here until one does. */}
      <ContestTeaser />
      <NeverSays />
    </>
  );
}
