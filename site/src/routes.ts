// SPDX-License-Identifier: Apache-2.0
//! The site's pages.
//!
//! A flat list rather than the console's audience-classified table, because
//! every page here is public by construction: this application is served as
//! static files from a host that has no idea who is reading. There is nothing to
//! classify and nothing that could leak.
//!
//! That is the whole argument for it being a separate application. In `web/`,
//! a page added to the routes and forgotten in `access::audience_of` is not a
//! 404 -- it is a page that silently requires operator identity. Here there is
//! no such seam to get wrong.

/** One page. */
export interface Route {
  readonly path: string;
  readonly label: string;
  /**
   * What the header calls it, when that differs.
   *
   * "Prize pool" wrapped onto two lines at 375px and made the sticky header
   * eat a third of the screen — on the width most of this traffic arrives at,
   * since the whole distribution is a link in a reply on a phone. The page's
   * own heading still says "The prize pool", so nothing is lost.
   */
  readonly short?: string;
  /**
   * Whether it appears in the header.
   *
   * `false` does not mean hidden. It means the footer, which is where a reader
   * looks for a privacy policy, terms, or a way to reach whoever runs the
   * thing — and it keeps the header at six items on a 375px phone, which is
   * the width most of this traffic arrives at.
   *
   * Both lists are derived from this table rather than written out again, so
   * the way to lose a page is to leave it out of the table entirely. A page in
   * the table with no `<Route>` in `App.tsx` renders the 404, and
   * `routes.test.tsx` fails on it.
   */
  readonly inNav: boolean;
}

/**
 * Paths this site used to serve, and where each one lives now (design 0025
 * §4). Links to the old paths are already posted in replies on X, and a reply
 * cannot be edited, so every old path keeps working as a redirect rather than
 * landing a stranger on "No such page".
 */
export const MOVED = [
  // The weekly prize is retired (ADR 0038): there is no live leaderboard or
  // pool any more, only the historical record of the weeks that ran while it
  // was live. Every old link that used to reach a live page now reaches that
  // record instead of a 404.
  { from: "/leaderboard", to: "/payouts" },
  { from: "/contest", to: "/payouts" },
  { from: "/pool", to: "/payouts" },
  { from: "/history", to: "/payouts" },
  { from: "/token", to: "/tokenomics" },
] as const;

export const ROUTES = [
  { path: "/", label: "Home", inNav: true },
  // The live contest and its prize pool are gone (ADR 0037, ADR 0038). What
  // is left at the same address is the historical record: the weeks that
  // closed while the prize ran, kept rather than deleted.
  { path: "/payouts", label: "History", inNav: true },
  { path: "/how-it-works", label: "How it works", short: "How", inNav: true },
  { path: "/tokenomics", label: "Tokenomics", short: "Token", inNav: true },
  { path: "/about", label: "About", inNav: true },
  // The three trust pages. Footer, not header: a stranger looks for these
  // before deciding whether to believe the rest of the site, and a young
  // domain talking about tokens without any of them reads to a reputation
  // classifier -- and to a person -- exactly the way it reads.
  { path: "/privacy", label: "Privacy", inNav: false },
  { path: "/terms", label: "Terms of use", short: "Terms", inNav: false },
  { path: "/contact", label: "Contact", inNav: false },
] as const satisfies readonly Route[];

/** The pages the header shows. */
export function nav(): readonly Route[] {
  return ROUTES.filter((r) => r.inNav);
}

/**
 * The pages the footer shows.
 *
 * The complement of [`nav`], deliberately: every page is in exactly one of the
 * two, so a page cannot be added to the table and then be reachable only by
 * typing its address.
 */
export function footer(): readonly Route[] {
  return ROUTES.filter((r) => !r.inNav);
}
