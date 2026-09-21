// SPDX-License-Identifier: Apache-2.0
//! What this is, who runs it, and what it will never say.
//!
//! # This page is a launch blocker, not a footer link
//!
//! X's policy requires an automated account to disclose that it is automated
//! and to identify who operates it. Design 0007 carries this as item B7 and
//! gates the account going live on it.
//!
//! It is also the page that makes the rest of the site checkable. Every claim
//! elsewhere is a number; this is where the method behind the numbers, and the
//! things deliberately not claimed, are written down.

import { Link } from "wouter";

import { useTitle } from "./title";
import { Block, Card, Heading, Section } from "./ui";

export function About() {
  useTitle("About");
  return (
    <Section>
      <Heading kicker="About">An automated account that measures things</Heading>

      <div className="max-w-2xl">
        <Card className="mb-10 border-[var(--color-edge)]">
          <p className="text-[var(--color-text)]">
            <strong>Real or Rug is an automated account.</strong> It is
            operated by Josh Fair. It never picks a coin to post about — every
            coin it names, somebody asked it about — it reports what it
            measured on chain, and it is not financial advice.
          </p>
        </Card>

        <Block title="What it does">
          <p>
            When you mention it with a mint address or a ticker, it reads the
            chain at that moment: the token's launch block, its bonding curve,
            and every other token the same creator has launched since Real or
            Rug started watching in August. Then it answers in the thread with
            what it found.
          </p>
          <p>
            The launch block is the part nobody else shows you. It is the first
            block of a coin's life, and if capital was committed before the coin
            existed, that is where the evidence sits.
          </p>
        </Block>

        <Block title="What it will never say">
          <p>
            It will never tell you a coin will go up, or that you should buy or
            sell anything. It does not predict prices and it does not rank coins
            by how good an investment they are.
          </p>
          <p>
            The reason is not caution, it is arithmetic. Nothing measured here
            supports a claim about where a price is going — and the strongest
            finding in the whole dataset is a{" "}
            <em>reason to refuse</em>, not a reason to buy.
          </p>
        </Block>

        <Block title="Where the numbers come from">
          <p>
            Every launch is recorded from the chain directly, by
            matching the program's instruction bytes — never a vendor's parsed
            feed and never a logged instruction name, both of which have been
            wrong here before. The outcomes are measured again as each token
            ages.
          </p>
          <p>
            Graduation is split into two kinds, because they mean opposite
            things. A curve that fills over time is demand. A curve bought out
            within three slots of launch was bought by money that was committed
            before the token existed.
          </p>
          <p>
            Every figure on this site is published with the moment it was
            measured, and it is expected to move. An earlier measurement of these
            same quantities was wrong by 2.7× nine days later, which is why no
            number here is presented as a constant.
          </p>
        </Block>

        <Block title="When it is wrong">
          <p>
            It will be. When a figure here turns out to be wrong, the correction
            is published in the same place as the original, and the account posts
            it. Corrections are not quietly edited in.
          </p>
          <p>
            If a coin is being described unfairly, or a number looks wrong, reply
            to the account and say so. The evidence behind every reply is kept,
            so a disagreement can be settled by looking rather than arguing.
          </p>
        </Block>

        <Block title="What it costs you">
          <p>
            Nothing. There is no token you need to hold, no subscription, and
            nothing to connect. Asking the account a question has never
            required owning anything, and it still does not.
          </p>
        </Block>

        <Block title="The token">
          <p>
            The token launches through pump.fun, on Solana, paired with SOL. It
            has not launched yet — there is no address to check and no price to
            quote. Creator fees go to a disclosed project treasury and pay
            disclosed operating costs and a reserve, nothing else. Holding the
            token gets you nothing from the product: no revenue share, no
            yield, no buyback, no prize, no preferential verdict. It is not an
            investment.
          </p>
          <p>
            The bot answers questions about that token on exactly the same rule
            as any other coin, and it never mentions its price.
          </p>
          <p>
            <Link
              href="/tokenomics"
              className="text-[var(--color-signal)] underline underline-offset-4 hover:text-[var(--color-text)]"
            >
              The plan, the wallets, and the risks →
            </Link>
          </p>
        </Block>

        <p className="mt-12 text-sm text-[var(--color-faint)]">
          Measured, not predicted. Not financial advice, not a recommendation,
          and not a solicitation to buy or sell anything.
        </p>
      </div>
    </Section>
  );
}
