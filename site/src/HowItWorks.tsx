// SPDX-License-Identifier: Apache-2.0
//! How a verdict is decided, and what the account will never do.
//!
//! The page a stranger needs before trusting a verdict (design 0025 §4e). It is
//! about the product's rule, not about who runs it -- that is `/about` -- and it
//! carries no live data, so it can never be stuck loading and never be wrong
//! about a number.
//!
//! The five levels are ADR 0027's ladder, named exactly as the code names them,
//! in the order the code ranks them. The wording beside each is this page's;
//! the names and the order are the decision, and a reader comparing a reply's
//! stamp to this page should find the same five.

import { Link } from "wouter";

import { useTitle } from "./title";
import { Block, Heading, Section } from "./ui";

/** One rung of the ladder. */
interface Level {
  readonly code: string;
  readonly stamp: string;
  /** A `--color-stamp-*` token: the ladder's colour on paper. */
  readonly ink: string;
  readonly means: string;
  readonly note: string;
}

const LADDER: readonly Level[] = [
  {
    code: "Rugged",
    stamp: "Rugged",
    ink: "var(--color-stamp-rug)",
    means: "It already happened, and the chain shows it.",
    note: "Liquidity removed, the creator sold out, or buyers cannot sell. An observation, not a forecast.",
  },
  {
    code: "RugMechanicsLive",
    stamp: "Rug mechanics live",
    ink: "var(--color-stamp-rug)",
    means: "Several strong warning signs are present right now.",
    note: "Never reached on one sign alone. Every sign has an innocent twin, so this level needs a combination.",
  },
  {
    code: "Sketchy",
    stamp: "Sketchy",
    ink: "var(--color-stamp-warn)",
    means: "Real red flags, with innocent explanations still open.",
    note: "A reason to look closer, not a finding that anybody did anything.",
  },
  {
    code: "NothingUglyYet",
    stamp: "Nothing ugly yet",
    ink: "var(--color-stamp-real)",
    means: "No bad sign found so far.",
    note: "The “yet” is part of the verdict. A young launch that looks clean is young, not safe.",
  },
  {
    code: "CantTell",
    stamp: "Can't tell",
    ink: "var(--color-stamp-unknown)",
    means: "A fact the verdict needed could not be read.",
    note: "Never shown as clean. Not knowing is not the same as nothing being wrong.",
  },
];

export function HowItWorks() {
  useTitle("How it works");
  return (
    <Section>
      <Heading kicker="How it works">How a verdict is decided</Heading>

      <div className="max-w-2xl space-y-3 text-[var(--color-dim)]">
        <p>
          Ask the account about a token and it reads the chain at that moment.
          Code, not a language model, turns what it found into one of five
          verdicts. The model writes the sentence around the verdict, and it is
          not allowed to change which one it is, or to add a number the chain
          did not give it.
        </p>
      </div>

      <ol className="mt-12 grid gap-8 sm:grid-cols-2" aria-label="The five verdicts, worst first">
        {LADDER.map((level) => (
          <li key={level.code} className="paper pinned p-6 pt-8">
            {/* The stamp sits above the words, not beside them: "Rug mechanics
                live" beside a sentence left the sentence a two-word column on
                a phone. */}
            <span className="stamp text-lg" style={{ color: level.ink }}>
              {level.stamp}
            </span>
            <p className="typewriter mt-5 text-lg leading-snug">{level.means}</p>
            <p className="mt-4 border-t border-dashed border-[var(--color-paper-dim)] pt-3 text-sm text-[var(--color-paper-dim)]">
              {level.note}
            </p>
          </li>
        ))}
      </ol>

      <div className="mt-20 max-w-2xl">
        <Block title="What it will never do">
          <ul className="list-disc space-y-2 pl-5">
            <li>
              <strong className="text-[var(--color-text)]">
                Name a person as a scammer.
              </strong>{" "}
              A verdict describes a token and its launch. Who is behind a
              wallet is a claim the chain cannot prove, so the account does not
              make it.
            </li>
            <li>
              <strong className="text-[var(--color-text)]">
                State a price or a market cap.
              </strong>{" "}
              Those figures are removed before the model ever sees the facts,
              so a reply cannot contain them.
            </li>
            <li>
              <strong className="text-[var(--color-text)]">
                Tell you to buy or sell.
              </strong>{" "}
              It reports what it measured. Nothing here is financial advice.
            </li>
            <li>
              <strong className="text-[var(--color-text)]">
                Let a model move money.
              </strong>{" "}
              Contest payouts follow a fixed rule written in code, and the key
              that pays them can do nothing else.
            </li>
            <li>
              <strong className="text-[var(--color-text)]">
                Follow instructions hidden in a post or a token&apos;s name.
              </strong>{" "}
              Everything it reads is treated as evidence, never as a command.
            </li>
          </ul>
        </Block>

        <p className="text-sm text-[var(--color-faint)]">
          Who runs the account, and how it is paid for, is on{" "}
          <Link
            href="/about"
            className="text-[var(--color-signal)] underline underline-offset-4 hover:text-[var(--color-text)]"
          >
            the about page
          </Link>
          .
        </p>
      </div>
    </Section>
  );
}
