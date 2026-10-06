// SPDX-License-Identifier: Apache-2.0
//! What this site collects, and the parts it cannot speak for.
//!
//! # Privacy includes the optional authenticated contribution workflow
//!
//! Two reasons, and the first is the smaller one. A young domain using crypto
//! vocabulary, with a leaderboard, a prize and a token, and with no privacy
//! policy, no terms and no way to reach whoever runs it, is the textbook input
//! to a reputation classifier. Guardio flagged `cabalhunter.org` on 2026-09-08.
//! These pages do not clear that on their own — an appeal does — but their
//! absence is the part a stranger can see.
//!
//! The larger reason is that the operator intends to take money eventually, and
//! a page like this is owed before that rather than after it.
//!
//! # The hard part is the second half, not the first
//!
//! Earlier browsing-only copy claimed no collection. The Library now submits
//! public research and uses the existing authenticated session for contributions.
//! The page must describe both workflows and provider processing. Fonts remain
//! bundled and no advertising analytics are added.
//!
//! What could not be established from the repository is which switches are on
//! in the operator's Cloudflare account, because a dashboard setting is not a
//! file. That is recorded on the page as unknown rather than
//! guessed in the flattering direction.

import { ISSUES, SOURCE } from "./honesty";
import { useTitle } from "./title";
import { Block, Card, Heading, Here, Out, Section } from "./ui";

export function Privacy() {
  useTitle("Privacy");
  return (
    <Section>
      <Heading kicker="Privacy">What this site collects</Heading>

      <div className="max-w-2xl">
        <Card className="mb-10 border-[var(--color-edge)]">
          <p className="text-[var(--color-text)]">
            <strong>Library browsing is public; contributions use X sign-in.</strong> The API receives searches, token addresses and submitted questions. Investigation content becomes public. The code describing these flows is{" "}
            <Out href={SOURCE}>public</Out>. The forecasting game also uses X sign-in; its account records and deletion controls are described on <Here href="/game-privacy">the game's privacy page</Here>.
          </p>
        </Card>

        <Block title="Session cookies and no advertising analytics">
          <p>
            X sign-in uses secure session and short-lived OAuth cookies. The server stores an X identity and a hash of the session token; the browser uses that session for authenticated submissions and the forecasting game. Library forms keep their drafts in memory. The site has no advertising pixel, tag manager or session recorder. Game account controls are on <Here href="/game-privacy">its privacy page</Here>.
          </p>
          <p>
            The typeface is served from this site's own address rather than from
            a font service, so even loading the page does not tell a third party
            that you are reading it.
          </p>
        </Block>

        <Block title="What the host sees, because every host does">
          <p>
            The site is served as static files through Cloudflare. Any web host
            necessarily sees a request arrive: the address it came from, the
            page asked for, the browser that asked, and roughly when. That is
            how the web works and it is not something this site chooses or
            switches off.
          </p>
          <p>
            <strong>What this page will not do is guess.</strong> Cloudflare
            also offers the operator a traffic dashboard, and whether it is
            switched on is a setting inside an account rather than a file in the
            repository this page was built from. So it is written down as
            unknown. When it has been established, it will be stated here with
            the date it was checked, like every other figure on this site.
          </p>
        </Block>

        <Block title="The numbers on the page">
          <p>
            The figures come from small public JSON documents — the population
            statistics and the historical record of past weeks. Your browser
            asks for them from Radar's own server, which sees that request the
            same way any web server sees one.
          </p>
          <p>
            Library requests include the selected network, address or search terms. The API sees them and may receive an existing session cookie. It uses transient address-based rate limits. Historic statistics can show a labeled committed snapshot; the Library reports unavailable data instead of substituting invented cases.
          </p>
        </Block>

        <Block title="Token questions and contributions are public">
          <p>
            Opening a dossier sends its token address and network to the API. Submitting an investigation sends your question, wallet and transaction leads. These become part of a durable public case history, including corrections. Do not include secrets or private personal information. Private admission records use a derived X-account identifier to enforce quotas across X and the website; that identifier is excluded from public case responses.
          </p>
        </Block>

        <Block title="The one place a name can appear">
          <p>
            If you mention the account on X, you have posted in public, and X's
            own terms govern that post. While the weekly prize ran, the
            past-weeks page republished what that public record holds: the
            numeric account id of an entrant, the handle if one was read at the
            close, the coin they asked about, and a link to the reply. Accounts
            that did not count are published as counts, never as names. That
            record stays up as history now that the prize has ended.
          </p>
          <p>
            Where a week paid a winner, the page links the transaction, because
            a payment nobody can check is not evidence of anything. The address
            is public because the chain is public, not because this site
            disclosed it.
          </p>
        </Block>

        <Block title="Account records and public history">
          <p>
            Sign-in uses X OAuth. Investigations may send public questions and chain evidence to the configured model provider. The operator retains case history and local backups; session/account deletion does not retract an already public case or X post. There is no mailing list or data-broker integration in this website. Contact the operator about account records or accidental disclosure.
          </p>
        </Block>

        <Block title="Links away from here">
          <p>
            Links to X and to a chain explorer take you to sites this operator
            does not run. What they collect is theirs to state, and their
            terms apply once you arrive.
          </p>
        </Block>

        <Block title="If this is wrong">
          <p>
            If you find anything on this site that gathers data and is not
            described above, that is a bug and it will be treated as one. Report
            it on <Out href={ISSUES}>the repository's issues</Out>, or reply to
            the account —{" "}
            <Here href="/contact">both are on the contact page</Here>
            .
          </p>
        </Block>

        <p className="mt-12 text-sm text-[var(--color-faint)]">
          Last reviewed 2026-10-01, against the source in this repository. This
          page changes by a public commit, so every version of it can be read
          alongside every other.
        </p>
      </div>
    </Section>
  );
}
