// SPDX-License-Identifier: Apache-2.0
//! The token, and every rule it is launched under.
//!
//! # pump.fun, on Solana, paired with SOL — and it has not launched
//!
//! ADR 0037 moves the token off Robinhood Chain and onto pump.fun. There is no
//! address, no dev buy, no wallet and no price to quote here yet, and this
//! page says so rather than showing a placeholder that could be mistaken for
//! one. When the token launches, the facts below are filled in, dated, and
//! never invented in the meantime.
//!
//! # There is no price on this page and there will never be one
//!
//! Not a stylistic choice. The bot never states the token's price or market
//! capitalisation, on any page, for any token including its own — and a
//! marketing page that printed what the bot is forbidden to say would make
//! that rule decorative.
//!
//! # No prize, no yield, no benefit from holding it
//!
//! ADR 0038 retires the weekly prize and every other holder benefit. Creator
//! fees go to a disclosed project treasury and pay disclosed operating costs
//! — servers, data, model usage — and a reserve. Nothing else. Holding the
//! token does not change a verdict, does not pay a return, and is not
//! required to use the bot for anything.

import { useTitle } from "./title";
import { Card, Heading, Nothing, Out, Section, Steps } from "./ui";

/**
 * The rules the token launches under.
 *
 * One sentence each, in the order a reader deciding in four seconds needs
 * them: what it is, what it is not, and what happens to the money.
 */
const RULES: readonly { readonly rule: string; readonly plain: string }[] = [
  {
    rule: "It launches through pump.fun, on Solana, paired with SOL.",
    plain:
      "Not Robinhood Chain, not any other chain, and not launched yet. There is no address to check until it is.",
  },
  {
    rule: "Project-controlled wallets are published with their addresses.",
    plain:
      "Any developer purchase or compensation is disclosed the same way — with its size, its wallet and its transaction, from launch day, not before.",
  },
  {
    rule:
      "Creator fees pay disclosed operating costs and a reserve. Nothing else.",
    plain:
      "Servers, data and model usage, published as categories. No prize, no buyback, no yield, no revenue share to anybody who holds the token.",
  },
  {
    rule: "Holding the token buys nothing in the product.",
    plain:
      "Every answer the bot gives is free to everyone, with or without it. It is not a share, it grants no vote, and it changes no verdict.",
  },
  {
    rule: "The bot never states the token's price or market capitalisation.",
    plain: "Not once, not if asked, not on this page either.",
  },
  {
    rule: "The token is judged like any other.",
    plain:
      "Same rule, same fact sheet, same refusals — including about its own launch and its own wallets. Ask it.",
  },
];

function Rules() {
  return (
    <Section id="rules">
      <Heading kicker="The six rules">What it is launched under</Heading>
      <p className="mb-8 max-w-2xl text-[var(--color-dim)]">
        A badge, not an investment. It is not a share, it does not grant a
        vote, and it buys no feature — every answer the bot gives is free to
        everyone, with or without it. Nothing here buys, sells or swaps the
        token automatically; any future trading needs its own decision, in
        public, first.
      </p>
      <div className="grid gap-4 md:grid-cols-2">
        {RULES.map((r, i) => (
          <Card key={i}>
            <div
              className="tnum font-mono text-xs text-[var(--color-signal-dim)]"
              aria-hidden="true"
            >
              {String(i + 1).padStart(2, "0")}
            </div>
            <p className="mt-2 font-medium text-[var(--color-text)]">{r.rule}</p>
            <p className="mt-2 text-sm text-[var(--color-dim)]">{r.plain}</p>
          </Card>
        ))}
      </div>
    </Section>
  );
}

function Money() {
  return (
    <Section id="money">
      <Heading kicker="Where the money goes">
        A fee on trading pays disclosed costs, and nothing else
      </Heading>
      <div className="max-w-2xl">
        <Steps
          steps={[
            {
              what: "Somebody trades the token on pump.fun. A creator fee is charged on the trade, in SOL.",
            },
            {
              what: "The fee is credited to the project's own treasury wallet, address published at launch.",
            },
            {
              what: "The treasury pays disclosed operating costs — servers, data, model usage — and holds a reserve.",
            },
            {
              what: "Nothing is paid to holders. There is no prize, no buyback and no yield to distribute.",
            },
          ]}
        />
      </div>
    </Section>
  );
}

/** pump.fun's own fee schedule, linked rather than quoted. */
function Fees() {
  return (
    <Section id="fees">
      <Heading kicker="The fee">Set by pump.fun, not by this project</Heading>
      <p className="max-w-2xl text-[var(--color-dim)]">
        pump.fun's fee rate depends on the launch stage and the token's market
        capitalisation, and both change as the token trades. Rather than
        quote a number here that would be wrong by the time somebody reads
        it, this page links the source instead.
      </p>
      <p className="mt-4">
        <Out href="https://pump.fun/docs/fees">
          pump.fun's fee documentation →
        </Out>
      </p>
    </Section>
  );
}

/** What can go wrong, stated rather than buried in a footnote. */
function Risks() {
  return (
    <Section id="risks">
      <Heading kicker="Risks">What this page will not soften</Heading>
      <ul className="max-w-2xl list-disc space-y-3 pl-5 text-sm text-[var(--color-dim)]">
        <li>
          <strong className="text-[var(--color-text)]">
            It can go to zero.
          </strong>{" "}
          A memecoin with no revenue, no product entitlement and no promise
          behind it can lose all of its value, and most do.
        </li>
        <li>
          <strong className="text-[var(--color-text)]">
            This is not an investment.
          </strong>{" "}
          Nothing here is an offer, a solicitation, or advice to buy, sell or
          hold anything.
        </li>
        <li>
          <strong className="text-[var(--color-text)]">
            There is no promise of fee income.
          </strong>{" "}
          Fees exist only if the token trades, and the project makes no
          commitment about how much that will ever be.
        </li>
        <li>
          <strong className="text-[var(--color-text)]">
            The fee rate is not fixed.
          </strong>{" "}
          pump.fun sets it by launch stage and market cap, and can change how
          it sets it. See the link above rather than a number frozen here.
        </li>
      </ul>
    </Section>
  );
}

function Gate() {
  return (
    <Section id="gate">
      <Heading kicker="Before any of this happens">
        What has to be true first
      </Heading>
      <div className="grid gap-6 md:grid-cols-2">
        <Card>
          <p className="text-[var(--color-text)]">The demand gate</p>
          <ul className="mt-3 space-y-2 text-sm text-[var(--color-dim)]">
            <li>· 30 days of the bot live and answering.</li>
            <li>· 200 distinct accounts that have summoned it.</li>
            <li>· 10% of its replies drawing any engagement at all.</li>
          </ul>
          <p className="mt-3 text-sm text-[var(--color-faint)]">
            A token launched into no demand is the thing this account exists to
            point at.
          </p>
        </Card>
        <Card>
          <p className="text-[var(--color-text)]">The legal read</p>
          <p className="mt-3 text-sm text-[var(--color-dim)]">
            A precondition, not a follow-up. Two questions have to be answered
            by somebody qualified before anything is minted, and the answer may
            be no.
          </p>
        </Card>
      </div>
    </Section>
  );
}

function Status() {
  return (
    <Section id="status">
      <Heading kicker="Right now">Status</Heading>
      <div className="max-w-2xl space-y-6">
        <Nothing
          what="No token exists."
          why="Nothing has been minted, no contract address has been published, and any address claiming to be this token is not. When one exists it will be published here and in the account's own bio, and nowhere else."
        />
        <Nothing
          what="Project-controlled wallets have no addresses yet."
          why="Any developer purchase or compensation, and the treasury wallet that receives creator fees, are published here on launch day with their size, address and transaction — not before, and not as a guess in the meantime."
        />
      </div>
    </Section>
  );
}

export function Token() {
  useTitle("Tokenomics");
  return (
    <>
      <Section className="pt-16 pb-0">
        <div className="enter">
          <h1 className="display max-w-3xl text-[length:var(--text-display)] leading-[1.05] font-semibold text-balance">
            A badge, not an investment.
          </h1>
          <p className="mt-6 max-w-2xl text-[length:var(--text-lead)] text-[var(--color-dim)]">
            There is no token yet. When there is one, these are the rules it is
            launched under — written down first, so they can be held against it
            afterwards.
          </p>
        </div>
      </Section>
      <Status />
      <Rules />
      <Money />
      <Fees />
      <Risks />
      <Gate />
    </>
  );
}
