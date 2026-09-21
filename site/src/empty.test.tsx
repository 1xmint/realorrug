// SPDX-License-Identifier: Apache-2.0
//! What the pages say when they have nothing to say.
//!
//! **This is the most important test file on the site**, because these are the
//! two states the whole thing ships in today and they will hold for as long as
//! the X account and the token take.
//!
//! Both have a wrong version that looks completely right. An empty table says a
//! week ran and nobody engaged. `0.00 SOL` says a contest exists and pays
//! nothing. Neither is true, both are what a reader takes away, and neither
//! would fail a test that only checked the page rendered.

import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { About } from "./About";
import { Contact } from "./Contact";
import { FORBIDDEN_CLAIMS } from "./honesty";
import { History } from "./History";
import { Home } from "./Home";
import { HowItWorks } from "./HowItWorks";
import { Token } from "./Token";
import { Summon } from "./ui";

/** No endpoint, which is exactly production today. */
function withNoServer() {
  vi.stubGlobal(
    "fetch",
    vi.fn(() => Promise.reject(new Error("no server"))),
  );
}

beforeEach(withNoServer);
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("the home page before a token exists", () => {
  afterEach(() => {
    vi.unstubAllEnvs();
  });

  it("says the token has not launched when no address is configured", async () => {
    render(<Home />);
    await waitFor(() => {
      expect(screen.getByText(/has not launched/i)).toBeTruthy();
    });
    const { container } = render(<Home />);
    expect(container.textContent ?? "").not.toMatch(/0x[0-9a-fA-F]{40}/);
  });
});

describe("the tokenomics page before any token exists", () => {
  it("says no token exists, in words, rather than showing a zero", async () => {
    render(<Token />);
    expect(screen.getByText(/No token exists/i)).toBeTruthy();
    expect(
      screen.getByText(/any address claiming to be this token is not/i),
    ).toBeTruthy();
  });

  it("says project wallets have no addresses yet", () => {
    // ADR 0037: no project-controlled wallet, dev buy or treasury address
    // exists before launch, and a placeholder that looked like an address
    // would be exactly the invented fact this page exists to refuse.
    render(<Token />);
    expect(
      screen.getByText(/Project-controlled wallets have no addresses yet/i),
    ).toBeTruthy();
    expect(
      screen.getByText(/published here on launch day/i),
    ).toBeTruthy();
  });

  it("puts no valuation of its own token on the page", () => {
    // ADR 0013 constraint 5 forbids the *bot* from stating the token's price or
    // market capitalisation. A marketing page printing what the bot is
    // forbidden to say would make the constraint decorative, so the same line
    // is held here -- by a test, because this is the page where the pressure to
    // add a number will come from.
    //
    // The check is a dollar figure, not the words. A first version banned
    // "market cap" outright and failed on the fee table, whose rows are keyed
    // on market capitalisation -- of whichever coin is being traded, not of
    // this token. Describing somebody else's fee schedule is not valuing your
    // own token, and a check that cannot tell those apart fires on a correct
    // page. Every figure here is in basis points or SOL, and a dollar sign is
    // how a valuation would arrive.
    const { container } = render(<Token />);
    const text = container.textContent ?? "";
    expect(text).not.toMatch(/\$\s?[\d.]/);
    // And it still says, in words, that the bot will not state the price.
    expect(text).toMatch(/never states the token's price/i);
  });

  it("links pump.fun's own fee documentation rather than quoting a rate", () => {
    // pump.fun's fee depends on launch stage and market cap, and both move.
    // A number typed onto this page would be wrong before somebody reads it,
    // so it links the source instead of quoting one.
    const { container } = render(<Token />);
    const link = screen.getByText(/pump\.fun's fee documentation/i);
    expect(link.closest("a")?.getAttribute("href")).toBe(
      "https://pump.fun/docs/fees",
    );
    expect(container.textContent ?? "").not.toMatch(/\d+\s?bps/);
  });

  it("states the risks rather than softening them", () => {
    render(<Token />);
    expect(screen.getByText(/It can go to zero/i)).toBeTruthy();
    expect(screen.getByText(/This is not an investment/i)).toBeTruthy();
    expect(
      screen.getByText(/There is no promise of fee income/i),
    ).toBeTruthy();
  });
});

describe("no page delivers a verdict", () => {
  // The bot is forbidden from saying a coin is a scam or that it is safe.
  // `forbidden.rs` enforces that on every reply. The site is the same product
  // speaking to the same strangers, and nothing enforced it here at all.
  //
  // Word-bounded, so "safety" and "farmer" do not fire, and applied to rendered
  // text rather than to source, so a word arriving through a fixture is caught
  // too.
  const FORBIDDEN = [
    "scam",
    "rug",
    "fraud",
    "sybil",
    "fake",
    "legit",
    "safe",
    "guaranteed",
  ];

  const pages: readonly [string, () => React.ReactElement][] = [
    ["home", () => <Home />],
    ["token", () => <Token />],
    ["about", () => <About />],
  ];

  for (const [name, page] of pages) {
    it(`${name} says none of them`, async () => {
      const { container } = render(page());
      await waitFor(() => expect(container.textContent).toBeTruthy());
      const text = (container.textContent ?? "").toLowerCase();
      for (const word of FORBIDDEN) {
        expect(
          new RegExp(`\b${word}\b`).test(text),
          `${name} contains the word "${word}"`,
        ).toBe(false);
      }
    });
  }
});

describe("no page invents a live prize or holder benefit", () => {
  // ADR 0038 retires the weekly prize and every other holder benefit. These
  // pages have no reason to ever mention one -- unlike About, Token, Terms,
  // Privacy and History, which state the retirement in words and so
  // legitimately contain "prize" and "payout" inside a sentence that denies
  // them. A blanket ban across every page would fail on the correct copy;
  // this list is the pages where the words should never appear at all.
  const pages: readonly [string, () => React.ReactElement][] = [
    ["home", () => <Home />],
    ["how it works", () => <HowItWorks />],
    ["contact", () => <Contact />],
  ];

  for (const [name, page] of pages) {
    it(`${name} names no prize, payout, buyback, yield or holder earnings`, async () => {
      const { container } = render(page());
      await waitFor(() => expect(container.textContent).toBeTruthy());
      const text = (container.textContent ?? "").toLowerCase();
      for (const claim of FORBIDDEN_CLAIMS) {
        expect(text, `${name} contains "${claim}"`).not.toContain(claim);
      }
    });
  }
});

describe("the summon box", () => {
  it("says so plainly when the account's handle is not configured", () => {
    // Production today: no VITE_X_HANDLE, and no file in the repository records
    // the handle as a fact. Rendering a guessed one would send a stranger to
    // somebody else's profile.
    render(<Summon handle={null} />);
    expect(screen.getByText(/not announced here yet/i)).toBeTruthy();
    expect(screen.queryByPlaceholderText(/token address/i)).toBe(null);
  });

  it("builds a prefilled post once a real address is typed", () => {
    // fireEvent rather than user-event: one controlled input does not justify
    // another devDependency, and the component reads `e.target.value` either
    // way. The comment in ui/index.tsx about not installing a library before a
    // component needs one applies to test libraries too.
    render(<Summon handle="realorrug" />);
    const box = screen.getByPlaceholderText(/token address/i);

    // Nothing typed: no link, so the button is absent rather than dead.
    expect(screen.queryByRole("link")).toBe(null);

    // Something that is not an address: the reader is told here, before it
    // costs them a public post that gets no answer.
    fireEvent.change(box, { target: { value: "not an address" } });
    expect(
      screen.getByText(/not shaped like a token address on either chain/i),
    ).toBeTruthy();
    expect(screen.queryByRole("link")).toBe(null);

    // A real one: the intent link, with the mint encoded into it.
    const mint = "HWvHqvfFVQdLZ1K3kMygpvhivVZEcrzVShgJFgtXpump";
    fireEvent.change(box, { target: { value: mint } });
    const link = screen.getByRole("link");
    expect(link.getAttribute("href")).toBe(
      `https://x.com/intent/post?text=%40realorrug%20${mint}`,
    );
  });

  it("summons about a Robinhood Chain token too, not only a Solana one", () => {
    // The box gated on base58 alone until 2026-09-17, so the chain the bot
    // mainly answers about got the refusal line and no button, from the front
    // page's own call to action. Re-apply the bug by dropping `evmShaped` from
    // `Summon` and this fails on the missing link.
    render(<Summon handle="realorrug" />);
    const box = screen.getByPlaceholderText(/token address/i);

    const token = "0x13e6cdB0470B10AfCB96177Ae8702ace2ac72cD6";
    fireEvent.change(box, { target: { value: token } });
    expect(screen.queryByText(/not shaped like a token address/i)).toBe(null);
    expect(screen.getByRole("link").getAttribute("href")).toBe(
      `https://x.com/intent/post?text=%40realorrug%20${token}`,
    );
  });
});

describe("the history page", () => {
  /** A server that answers `/weeks` with one closed, claimed and paid week. */
  function withWeeks(week: unknown) {
    vi.stubGlobal(
      "fetch",
      vi.fn((url: string) =>
        String(url).includes("/weeks")
          ? Promise.resolve({
              ok: true,
              json: () =>
                Promise.resolve({
                  measured_at: "2026-09-07T00:01:00Z",
                  weeks: [week],
                }),
            })
          : Promise.reject(new Error("no server")),
      ),
    );
  }

  const paid = {
    week: "2026-08-31",
    opened_at: "2026-08-31T00:00:00Z",
    closed_at: "2026-09-07T00:00:00Z",
    entries: 4,
    excluded: { count: 2, reasons: { account_too_new: 1, operator: 1 } },
    winner: {
      summoner: "1889496824328880128",
      handle: "somebody",
      reply_url: "https://x.com/i/web/status/1",
      score: 12,
      mint: "So11111111111111111111111111111111111111112",
      verified: {
        reposts: 4,
        quoters: 1,
        likes: 6,
        engagers: 9,
        engagers_under_age: 2,
      },
    },
    rule: {
      operators: 2,
      min_account_age_days: 30,
      min_engager_age_days: 30,
      cooldown_weeks: 3,
    },
    voided: null,
    claim: {
      state: "claimed",
      at: "2026-09-07T00:05:00Z",
      address: "So11111111111111111111111111111111111111112",
      reply_url: "https://x.com/i/web/status/2",
    },
    payout: {
      state: "paid",
      lamports: 3_000_000_000_000_000_000,
      recipient: "0xb0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0",
      signature:
        "0x5f3a1c2d4e6b7890abcdef1234567890abcdef1234567890abcdef1234567890",
      at: "2026-09-07T01:00:00Z",
    },
  };

  it("says no week has closed rather than showing an empty table", async () => {
    // The state it ships in today, and the wrong version looks right: an empty
    // table says a week ran and nobody entered it.
    render(<History />);
    expect(await screen.findByText(/No week has closed yet/i)).toBeTruthy();
    expect(document.querySelector("table")).toBeNull();
  });

  it("renders a paid week as three things a stranger can check", async () => {
    withWeeks(paid);
    render(<History />);

    // The winner, by handle, linked by id (S4/S27).
    const winner = await screen.findByText("@somebody");
    expect(winner.getAttribute("href")).toBe(
      "https://x.com/i/user/1889496824328880128",
    );
    // The winning reply, the claim, and the transaction: all links out.
    const links = Array.from(document.querySelectorAll("a")).map((a) =>
      a.getAttribute("href"),
    );
    expect(links).toContain("https://x.com/i/web/status/1");
    expect(links).toContain("https://x.com/i/web/status/2");
    expect(
      links.some((h) =>
        h?.startsWith("https://robinhoodchain.blockscout.com/tx/"),
      ),
    ).toBe(true);
    // The prize, in the unit it was paid in.
    expect(screen.getByText(/3\.0000 ETH/)).toBeTruthy();
    // The rule the week was scored under, printed beside it.
    expect(document.body.textContent).toContain("Scored under");
    // And the exclusions as counts, never as names.
    expect(document.body.textContent).toContain(
      "account younger than the rule allows",
    );
  });

  it("says why a week paid nobody instead of leaving the cell blank", async () => {
    // Four different facts. A page that renders all four the same asks the
    // reader to trust the operator about the one thing they would not.
    withWeeks({
      ...paid,
      winner: { ...paid.winner, verified: null },
      claim: { state: "rolled_over", closed_at: "2026-09-14T00:00:00Z" },
      payout: { state: "unclaimed" },
    });
    render(<History />);
    expect(await screen.findByText(/never claimed/i)).toBeTruthy();
    expect(document.body.textContent).not.toContain("ETH");
    // Unread engagement is "not read", never a row of zeroes.
    expect(document.body.textContent).toContain("not read");
    expect(document.body.textContent).not.toContain("0/0/0");
  });

  it("publishes the reason a week was voided, verbatim", async () => {
    // Design 0011: the correction is public or it is not a correction.
    withWeeks({
      ...paid,
      voided: {
        at: "2026-09-08T00:00:00Z",
        reason: "every point came from six accounts made that morning",
      },
      payout: { state: "voided" },
    });
    render(<History />);
    expect(
      await screen.findByText(/six accounts made that morning/),
    ).toBeTruthy();
    expect(document.body.textContent).toContain("pays nobody");
  });

  it("shows no rule at all for a week that did not record one", async () => {
    // Rule 9 on the page somebody opens to dispute a placing. Today's numbers
    // are not evidence about a week that closed before they existed.
    withWeeks({ ...paid, rule: null });
    render(<History />);
    expect(await screen.findByText(/was not recorded/i)).toBeTruthy();
    expect(document.body.textContent).not.toContain("Scored under");
  });
});
