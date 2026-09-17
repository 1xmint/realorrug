// SPDX-License-Identifier: Apache-2.0
//! The token, and every rule it is launched under.
//!
//! # There is no price on this page and there will never be one
//!
//! Not a stylistic choice. [ADR 0013](../../docs/adr/0013-a-community-token-exists-and-radar-holds-none-of-it.md)
//! constraint 5 forbids the *bot* from stating the token's price or market
//! capitalisation, and a marketing page that prints what the bot is forbidden to
//! say would make that constraint decorative. `REALORRUG_SELF_MINT` enforces it on
//! the Rust side by refusing to answer about the token with any price fact; this
//! page holds the same line by having nothing of the kind to render.
//!
//! What is here instead is arithmetic a reader can check: the fee schedule, read
//! off Robinhood Chain, and where the fee goes. `fee-ladder.json` is a fixture
//! this repository checks by hand against
//! [research 0036](../../docs/research/0036-pons-v2-read-from-a-real-launch.md),
//! not a decoder-pinned file the way the old pump.fun ladder was — see that
//! file's own comment for the sources and the date.
//!
//! # ADR 0029: the dev buy and the bot's wallet, disclosed rather than absent
//!
//! [ADR 0029](../../docs/adr/0029-the-bot-holds-its-own-token-openly.md)
//! supersedes ADR 0013 constraints 1 and 2. There is one small dev buy, in the
//! launch block, and the bot's wallet may hold the token — both stated here
//! with their size, wallet and transaction from launch day, never before it.
//! Constraints 3 to 6 stand: the creator tax still becomes the whole prize,
//! entry is still free, the bot still never states a price, and the token is
//! still judged exactly like any other.
//!
//! # Why this page exists separately from the pool
//!
//! `/pool` answers "what is in the pot this week". This answers "what is this
//! thing and what stops you being the exit liquidity". They are different
//! questions from different readers, and the second one is the one a stranger
//! arriving from a reply actually has.

import ladder from "./fixtures/fee-ladder.json";
import { useTitle } from "./title";
import { Card, Heading, Nothing, Section, Steps } from "./ui";

/**
 * The rules it is launched under, in ADR 0029's own order — the two it
 * changed first, then the four ADR 0013 constraints it left standing.
 *
 * One sentence each. The ADRs argue them; this states them, because a reader
 * deciding in four seconds needs the constraint, not the reasoning.
 */
const RULES: readonly { readonly rule: string; readonly plain: string }[] = [
  {
    rule: "One small dev buy, in the launch block, stated in public.",
    plain:
      "Its size, the wallet that made it and the transaction are on this page from launch day. Before then there is no address, size or transaction yet.",
  },
  {
    rule: "The bot's wallet may hold the token, and its address is public.",
    plain: "Every token it holds is visible on chain to anyone who looks.",
  },
  {
    rule: "The creator tax goes to the bot's wallet and funds the weekly prize.",
    plain: "Nothing is kept back. It leaves again the same week.",
  },
  {
    rule: "Entry is free and never requires holding the token.",
    plain: "Mention the account with a coin. That is the whole entry.",
  },
  {
    rule: "The bot never states the token's price or market capitalisation.",
    plain: "Not once, not if asked, not on this page either.",
  },
  {
    rule: "The token is judged like any other.",
    plain: "Same rule, same fact sheet, same refusals — including the dev buy this page discloses. Ask it.",
  },
];

function Rules() {
  return (
    <Section id="rules">
      <Heading kicker="The six rules">What it is launched under</Heading>
      <p className="mb-8 max-w-2xl text-[var(--color-dim)]">
        A badge, not an investment. It is not a share, it does not grant a vote,
        and it buys no feature — every answer the bot gives is free to everyone,
        with or without it. Nothing here buys, sells or swaps the token
        automatically; any future trading needs its own decision, in public,
        first.
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
        Volume becomes a fee, and the fee becomes the prize
      </Heading>
      <div className="max-w-2xl">
        <Steps
          steps={[
            {
              what: "Somebody trades the token. A base fee and the creator tax are charged on the trade, in ETH.",
            },
            {
              what: "The creator's share is credited to a fee escrow the launchpad keeps, not paid out directly.",
            },
            {
              what: "The bot's own wallet — the creator fee recipient, address public — claims it. Nothing is kept back.",
            },
            {
              what: "The week's best question wins the pool, in one public transaction the payout wallet signs and nothing else.",
              when: "Mondays, 00:00 UTC close · payout 01:00 UTC",
            },
          ]}
        />
      </div>
    </Section>
  );
}

function Ladder() {
  const curve = ladder.curve;
  return (
    <Section id="fees">
      <Heading kicker="The fee, read off Robinhood Chain">
        100 basis points, split, plus a chosen tax
      </Heading>
      <p className="mb-6 max-w-2xl text-[var(--color-dim)]">
        While a coin is still on its bonding curve, every trade pays a base fee
        of{" "}
        <strong className="text-[var(--color-text)]">
          {curve.base_fee_bps} basis points
        </strong>{" "}
        of volume — 1%. The protocol keeps{" "}
        {curve.protocol_share_bps / 100}% of that fee; the creator gets the
        other {curve.creator_base_share_bps / 100}%. On top of that, the
        creator sets a tax once, at launch, from 0 up to{" "}
        {curve.creator_tax_ceiling_bps} basis points, paid to the creator in
        full. All of it is in ETH, and all of the creator's side is this
        token's weekly prize.
      </p>
      <Card>
        <dl className="grid grid-cols-2 gap-x-6 gap-y-3 text-sm sm:grid-cols-4">
          <div>
            <dt className="text-xs text-[var(--color-faint)]">Base fee</dt>
            <dd className="tnum mt-1 text-[var(--color-text)]">
              {curve.base_fee_bps} bps
            </dd>
          </div>
          <div>
            <dt className="text-xs text-[var(--color-faint)]">
              Creator's share of it
            </dt>
            <dd className="tnum mt-1 font-medium text-[var(--color-signal)]">
              {curve.creator_base_share_bps / 100}%
            </dd>
          </div>
          <div>
            <dt className="text-xs text-[var(--color-faint)]">
              Protocol's share of it
            </dt>
            <dd className="tnum mt-1 text-[var(--color-dim)]">
              {curve.protocol_share_bps / 100}%
            </dd>
          </div>
          <div>
            <dt className="text-xs text-[var(--color-faint)]">
              Creator tax ceiling
            </dt>
            <dd className="tnum mt-1 text-[var(--color-dim)]">
              {curve.creator_tax_ceiling_bps} bps
            </dd>
          </div>
        </dl>
      </Card>
      <p className="mt-4 max-w-2xl text-sm text-[var(--color-dim)]">
        {!ladder.after_graduation.established && (
          <>
            <strong className="text-[var(--color-text)]">
              What happens to the fee after graduation is not established
              here.
            </strong>{" "}
            A graduated Pons v2 pool is Uniswap-shaped, with a hook, and its
            fee split has not been read off chain — so this page states the
            curve's fee, which is what applies for as long as the coin trades
            on it, and says nothing about a number it has not checked.
          </>
        )}
      </p>
      <p className="mt-4 text-xs text-[var(--color-faint)]">
        Read from the launchpad's own factory on {ladder.captured}. The
        factory owner can change these settings for future launches; a
        launched curve keeps what it snapshotted, which is why this page is
        dated.
      </p>
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
          what="The dev buy has no size, wallet or transaction yet."
          why="ADR 0029: there is one small dev buy, in the launch block. Its size, the wallet that made it and the transaction are published on this page on launch day — not before, and not as a guess in the meantime."
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
      <Ladder />
      <Gate />
    </>
  );
}
