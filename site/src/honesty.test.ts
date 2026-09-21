// SPDX-License-Identifier: Apache-2.0
//! What this site is allowed to claim.
//!
//! Every case here is a sentence a stranger would read about somebody else's
//! project. That is why the empty and missing cases get as much attention as the
//! populated ones: the wrong version of each of these looks completely right.

import { describe, expect, it } from "vitest";

import {
  count,
  eth,
  evmShaped,
  explorerAccount,
  explorerTx,
  FORBIDDEN_CLAIMS,
  measuredAgo,
  handleHref,
  mintShaped,
  safeHref,
  solscanAccount,
  solscanTx,
  summonIntent,
  userHref,
} from "./honesty";

describe("measuredAgo", () => {
  const now = new Date("2026-09-05T00:00:00Z");

  it("says how stale each figure is, in words", () => {
    expect(measuredAgo("2026-09-05T00:00:00Z", now)).toBe("just now");
    expect(measuredAgo("2026-09-04T23:00:00Z", now)).toBe("60 minutes ago");
    expect(measuredAgo("2026-09-04T12:00:00Z", now)).toBe("12 hours ago");
    expect(measuredAgo("2026-08-30T00:00:00Z", now)).toBe("6 days ago");
  });

  it("says nothing rather than guessing at a broken timestamp", () => {
    // A clock skew rendering "in 3 hours" looks like a bug in the data, which
    // is worse for trust than an absent line.
    expect(measuredAgo("not a date", now)).toBeNull();
    expect(measuredAgo("2026-09-06T00:00:00Z", now)).toBeNull();
  });
});

describe("count", () => {
  it("separates thousands, because 508814 is unreadable", () => {
    expect(count(508814)).toMatch(/508.814/);
  });
});

describe("links", () => {
  // Refusals first, because a link helper that never returns null is a link
  // helper that is not doing anything.

  it("refuses a scheme that is not https", () => {
    // The one that matters. React warns on this and does not block it.
    expect(safeHref("javascript:alert(1)", ["x.com"])).toBe(null);
    expect(safeHref("data:text/html,<script>", ["x.com"])).toBe(null);
    expect(safeHref("http://x.com/a", ["x.com"])).toBe(null);
  });

  it("refuses a host that only looks like the allowed one", () => {
    // Every hand-rolled version of this check is a prefix match, and every one
    // of them passes this case.
    expect(safeHref("https://evil.example/#@x.com", ["x.com"])).toBe(null);
    expect(safeHref("https://x.com.evil.example/a", ["x.com"])).toBe(null);
    expect(safeHref("https://notx.com/a", ["x.com"])).toBe(null);
    expect(safeHref("not a url at all", ["x.com"])).toBe(null);
  });

  it("allows the host it was given", () => {
    expect(safeHref("https://x.com/realorrug", ["x.com"])).toBe(
      "https://x.com/realorrug",
    );
  });

  it("refuses a sixteenth character in a handle", () => {
    // X's own bound. A 16-character handle renders as a link to a profile that
    // does not exist, on the page introducing the account.
    expect(handleHref("a".repeat(15))).toBe(`https://x.com/${"a".repeat(15)}`);
    expect(handleHref("a".repeat(16))).toBe(null);
    expect(handleHref("")).toBe(null);
    expect(handleHref("has space")).toBe(null);
    expect(handleHref("has-dash")).toBe(null);
    expect(handleHref("@leading")).toBe(null);
  });

  it("refuses anything but digits in a user id", () => {
    expect(userHref("1234567890")).toBe("https://x.com/i/user/1234567890");
    expect(userHref("12a")).toBe(null);
    expect(userHref("")).toBe(null);
    expect(userHref("../../evil")).toBe(null);
  });

  it("refuses a signature with a character base58 does not have", () => {
    // '0' is excluded from base58 precisely because it is confusable with 'O',
    // and a signature containing one did not come off a chain.
    const good = "5".repeat(88);
    expect(solscanTx(good)).toBe(`https://solscan.io/tx/${good}`);
    expect(solscanTx(`0${"5".repeat(87)}`)).toBe(null);
    expect(solscanTx("5".repeat(85))).toBe(null);
    expect(solscanTx("5".repeat(89))).toBe(null);
  });

  it("reads a mint the way the bot's own parser does", () => {
    // 32 to 44, bounds exact -- mention.rs MIN_ADDRESS and MAX_ADDRESS. A
    // reader whose paste this rejects would have been refused by the bot too,
    // and the summon box should say so before it costs them a post.
    expect(mintShaped("a".repeat(32))).toBe(true);
    expect(mintShaped("a".repeat(44))).toBe(true);
    expect(mintShaped("a".repeat(31))).toBe(false);
    expect(mintShaped("a".repeat(45))).toBe(false);
    expect(mintShaped(`  ${"a".repeat(32)}  `)).toBe(true);
    expect(mintShaped(`0${"a".repeat(31)}`)).toBe(false);
    expect(solscanAccount("a".repeat(44))).toBe(
      `https://solscan.io/account/${"a".repeat(44)}`,
    );
    expect(solscanAccount("not an address")).toBe(null);
  });

  it("builds a summons only when both halves are real", () => {
    const mint = "a".repeat(43);
    expect(summonIntent("realorrug", mint)).toBe(
      `https://x.com/intent/post?text=%40realorrug%20${mint}`,
    );
    // A button that posts "@undefined <mint>" is worse than no button.
    expect(summonIntent("", mint)).toBe(null);
    expect(summonIntent("a".repeat(16), mint)).toBe(null);
    expect(summonIntent("realorrug", "not an address")).toBe(null);

    // And a Robinhood Chain address is a real half too. This gated on base58
    // alone until 2026-09-17; re-apply the bug by dropping `evmShaped` from
    // `summonIntent` and this returns null for the chain the bot mainly reads.
    const token = "0x13e6cdB0470B10AfCB96177Ae8702ace2ac72cD6";
    expect(summonIntent("realorrug", token)).toBe(
      `https://x.com/intent/post?text=%40realorrug%20${token}`,
    );
  });

  it("encodes the mint rather than pasting it into a query string", () => {
    // The mint is address-shaped by the time it reaches here, so nothing needs
    // escaping today. The encoding is asserted anyway: the day this function
    // takes a ticker instead, '$' and '&' arrive with it.
    const url = summonIntent("realorrug", "a".repeat(32));
    expect(url).not.toBe(null);
    expect(url).toContain("%40");
    expect(url).not.toContain("@");
  });

  it("refuses a transaction hash the wrong length or shape for Robinhood Chain", () => {
    // The token, the bot's wallet and the weekly payout are all Robinhood
    // Chain now (ADR 0029, ADR 0025) -- these are the links Pool.tsx and
    // History.tsx actually render.
    const good = `0x${"a".repeat(64)}`;
    expect(explorerTx(good)).toBe(
      `https://robinhoodchain.blockscout.com/tx/${good}`,
    );
    expect(explorerTx("a".repeat(64))).toBe(null);
    expect(explorerTx(`0x${"a".repeat(63)}`)).toBe(null);
    expect(explorerTx(`0x${"a".repeat(65)}`)).toBe(null);
    expect(explorerTx(`0x${"g".repeat(64)}`)).toBe(null);
  });

  it("refuses an account address the wrong length or shape for Robinhood Chain", () => {
    const good = `0x${"b".repeat(40)}`;
    expect(evmShaped(good)).toBe(true);
    expect(explorerAccount(good)).toBe(
      `https://robinhoodchain.blockscout.com/address/${good}`,
    );
    expect(explorerAccount(`0x${"b".repeat(39)}`)).toBe(null);
    expect(explorerAccount("not an address")).toBe(null);
  });
});

describe("eth", () => {
  it("formats wei as ETH to four decimal places", () => {
    // wei is 10^18, not Solana's 10^9 lamports -- a fixture written in the
    // old unit would render as a figure nine orders of magnitude too small.
    expect(eth(3_000_000_000_000_000_000)).toBe("3.0000");
    expect(eth(0)).toBe("0.0000");
    expect(eth(1_500_000_000_000_000)).toBe("0.0015");
  });
});

describe("FORBIDDEN_CLAIMS", () => {
  it("names the holder-benefit wording ADR 0038 retired", () => {
    // A fixed list, not a computed one: this is a decision (ADR 0038), and a
    // test that derived the list from itself would prove nothing changed.
    for (const claim of [
      "prize",
      "payout",
      "buyback",
      "holders earn",
      "yield",
    ]) {
      expect(FORBIDDEN_CLAIMS).toContain(claim);
    }
  });
});
