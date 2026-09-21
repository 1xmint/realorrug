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
//! token address (deny-by-default, see `TokenAddress.tsx`), and a short link
//! to `/how-it-works` rather than a restatement of it.
//!
//! # No contest teaser
//!
//! ADR 0038 retires the weekly prize, so there is no live leaderboard to tease
//! here any more. `/payouts` is now the historical record of the weeks that
//! ran while it did, not a page this page previews.
//!
//! # The live feed lists what was posted, and nothing else
//!
//! Design 0025 §4a item 5: a short list of recent verdicts, read from
//! `/v1/public/recent`, which lists only replies the account actually posted
//! with the level code picked for them. With nothing posted, or no server,
//! the section says so in words rather than showing an empty list or a
//! made-up one -- the LEARNINGS-5 failure this site works to avoid.

import { useEffect, useState } from "react";
import { Link } from "wouter";

import { recent as fetchRecent, type Recent } from "./api";
import { measuredAgo, safeHref } from "./honesty";
import { LADDER } from "./HowItWorks";
import { TokenAddress } from "./TokenAddress";
import { Card, Heading, Nothing, Section, CheckBox } from "./ui";

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
          Paste a token&apos;s contract address — Robinhood Chain or Solana —
          and Real or Rug reads the chain right then: the launch, the
          creator&apos;s history, what actually happened. Not a prediction, not
          advice: what the chain shows.
        </p>
        <CheckBox />
        <TokenAddress />
      </div>
    </Section>
  );
}

/** `0x22fd…48fa`: enough to tell two tokens apart, short enough to scan. */
export function shortAddress(address: string): string {
  return address.length > 12
    ? `${address.slice(0, 6)}…${address.slice(-4)}`
    : address;
}

/** The newest verdicts the account posted, each linking to its own check. */
function JustChecked() {
  const [data, setData] = useState<Recent | null>(null);

  useEffect(() => {
    let live = true;
    void fetchRecent().then((next) => {
      if (live) setData(next);
    });
    return () => {
      live = false;
    };
  }, []);

  return (
    <Section id="recent">
      <Act n="02">Just checked</Act>
      <Heading>The latest verdicts</Heading>
      {data === null ? null : data.verdicts.length === 0 ? (
        <Nothing
          what="No verdicts posted yet."
          why="Every reply the account posts about a token lands here, newest first, with the stamp it earned. Paste an address above to check one now."
        />
      ) : (
        <Card className="max-w-2xl">
          <ul className="space-y-3">
            {data.verdicts.map((v) => {
              const rung = LADDER.find((l) => l.code === v.level);
              const reply = safeHref(v.reply_url ?? "", ["x.com"]);
              return (
                <li
                  key={`${v.at}-${v.address}`}
                  className="flex flex-wrap items-center justify-between gap-x-4 gap-y-1 text-sm"
                >
                  <span className="flex items-center gap-3">
                    <span
                      className="stamp text-base"
                      style={{ color: rung?.ink }}
                    >
                      {rung?.stamp ?? v.level}
                    </span>
                    <Link
                      href={`/check/${encodeURIComponent(v.address)}`}
                      className="font-mono text-[var(--color-text)] underline underline-offset-4 hover:text-[var(--color-signal)]"
                    >
                      {shortAddress(v.address)}
                    </Link>
                  </span>
                  <span className="flex items-center gap-3 text-[var(--color-dim)]">
                    <span>{measuredAgo(v.at) ?? ""}</span>
                    {reply ? (
                      <a
                        href={reply}
                        target="_blank"
                        rel="noopener noreferrer"
                        className="underline underline-offset-4 hover:text-[var(--color-text)]"
                      >
                        the reply
                      </a>
                    ) : null}
                  </span>
                </li>
              );
            })}
          </ul>
        </Card>
      )}
    </Section>
  );
}

/** What it will never do, in one paragraph, with the page that says the rest. */
function NeverSays() {
  return (
    <Section id="never">
      <Act n="04">What it will never say</Act>
      <Heading>The refusals are the product</Heading>
      <p className="max-w-2xl text-[var(--color-dim)]">
        No price target, no "buy" or "sell", no "this one is safe". Real or Rug
        reports what the chain shows and lets the numbers say it — enforced in
        code, on every reply, before it is sent.
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
      <JustChecked />
      <NeverSays />
    </>
  );
}
