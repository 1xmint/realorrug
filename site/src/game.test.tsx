// SPDX-License-Identifier: Apache-2.0
//! The forecasting game's pages (design 0032 §13).
//!
//! Each test here is a promise the pages make to a stranger, and each has a
//! wrong version that renders cleanly:
//!
//! - **No API base.** A build with no game server must say the game is not
//!   running. The wrong version calls the site's own origin (a static host
//!   that answers every path with `index.html`) or draws sample data.
//! - **An open round.** The server hides other players' calls and their count
//!   until the window closes. The wrong version asks for them anyway, or draws
//!   "0 calls so far", which is a fact about the hidden calls even at zero.
//! - **A closed round's board.** Counts come with the sample size and the age
//!   of the reading, never a score, a rank, a winner or a total.
//! - **The form.** The API is a different origin from the page, so a fetch
//!   without `credentials: "include"` sends no session cookie, and a POST
//!   without the CSRF header is refused. Both fail only against a real server,
//!   so they are held here.
//!
//! Verified by re-applying the bugs; see design 0032 §13 for which.

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { Router } from "wouter";
import { memoryLocation } from "wouter/memory-location";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { App } from "./App";
import { CSRF_HEADER } from "./game";
import { gameCopyViolations } from "./honesty";

const API = "https://api.example.test";
const READ_AT = 1_790_000_000;
const NEWEST_AT = 1_789_999_900;
const CLOSE_AT = 1_789_990_000;
const READ_ISO = new Date(READ_AT * 1000).toISOString();
const NEWEST_ISO = new Date(NEWEST_AT * 1000).toISOString();

const COIN_A = { chain: "solana", token: "MintAaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" };
const COIN_B = { chain: "solana", token: "MintBbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb" };

interface Canned {
  readonly status?: number;
  readonly body: unknown;
}

interface Seen {
  readonly url: URL;
  readonly init: RequestInit | undefined;
}

/**
 * A fake game server. Keyed `"METHOD /path"`. Anything not listed is a 404, so
 * a page that asks for a route the test did not expect is visible in `seen`
 * and in what the page draws.
 */
function serve(table: Record<string, Canned>): Seen[] {
  const seen: Seen[] = [];
  vi.stubGlobal(
    "fetch",
    vi.fn((input: RequestInfo | URL, init?: RequestInit) => {
      const url = new URL(String(input));
      seen.push({ url, init });
      const canned = table[`${init?.method ?? "GET"} ${url.pathname}`];
      if (canned === undefined) {
        return Promise.resolve(new Response(JSON.stringify({ error: "not found" }), { status: 404 }));
      }
      return Promise.resolve(
        new Response(JSON.stringify(canned.body), { status: canned.status ?? 200 }),
      );
    }),
  );
  return seen;
}

function renderAt(path: string) {
  const { hook } = memoryLocation({ path, static: true });
  return render(
    <Router hook={hook}>
      <App />
    </Router>,
  );
}

/** Everything the page has drawn, as one string. */
function drawn(container: HTMLElement): string {
  return (container.querySelector("main")?.textContent ?? "").replace(/\s+/g, " ");
}

const SIGNED_OUT: Record<string, Canned> = {
  "GET /auth/me": { status: 401, body: { error: "not signed in" } },
};

const SIGNED_IN: Record<string, Canned> = {
  "GET /auth/me": {
    body: { signed_in: true, handle: "someone", csrf_token: "csrf-abc" },
  },
};

const OPEN_ROUND: Canned = {
  body: {
    round: "r1",
    window_close: CLOSE_AT,
    closed: false,
    coins: [COIN_A, COIN_B],
    read_at: READ_AT,
    newest_at: null,
  },
};

const CLOSED_ROUND: Canned = {
  body: { ...(OPEN_ROUND.body as object), closed: true, newest_at: NEWEST_AT },
};

beforeEach(() => {
  vi.stubEnv("VITE_API_BASE", API);
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  vi.unstubAllEnvs();
});

describe("with no API base the game is not running", () => {
  const PAGES = ["/play", "/play/r1", "/board", "/my-calls", "/my-calls/r1", "/game-privacy"];

  for (const path of PAGES) {
    it(`${path} says so, draws nothing else and asks nobody`, async () => {
      vi.stubEnv("VITE_API_BASE", "");
      const seen = serve({});
      const { container } = renderAt(path);
      expect(await screen.findByText(/game is not running/i)).toBeTruthy();
      const text = drawn(container);
      // Not an empty round, not a board of zeroes, not a form.
      expect(container.querySelector("table")).toBeNull();
      expect(text).not.toMatch(/demo round|example round|sample round/i);
      expect(screen.queryByRole("button", { name: /call .* real/i })).toBeNull();
      // Deny by default: with no base, not even the site's own origin is asked.
      expect(seen).toHaveLength(0);
    });
  }

  it("a base that is not https (and not loopback) is off, not repaired", async () => {
    vi.stubEnv("VITE_API_BASE", "http://api.example.test");
    const seen = serve({});
    renderAt("/board");
    expect(await screen.findByText(/game is not running/i)).toBeTruthy();
    expect(seen).toHaveLength(0);
  });
});

describe("a closed round's board", () => {
  it("shows hit and miss counts with n per player and the age of the reading", async () => {
    serve({
      "GET /v1/board": {
        body: {
          board: [
            { board_id: "p-7f3a", hits: 4, misses: 1, n: 5 },
            { board_id: "p-91c2", hits: 9, misses: 2, n: 11 },
          ],
          read_at: READ_AT,
          newest_at: NEWEST_AT,
        },
      },
    });
    const { container } = renderAt("/board");
    expect(await screen.findByText("p-7f3a")).toBeTruthy();

    const rows = Array.from(container.querySelectorAll("tbody tr")).map((tr) =>
      Array.from(tr.children).map((c) => c.textContent),
    );
    // In the order the server sent them: a sort by hits is the first step to a winner.
    expect(rows).toEqual([
      ["p-7f3a", "4", "1", "5"],
      ["p-91c2", "9", "2", "11"],
    ]);
    const headers = Array.from(container.querySelectorAll("thead th")).map((h) => h.textContent);
    expect(headers).toEqual(["Player id", "Hits", "Misses", "Settled calls (n)"]);

    const age = container.querySelector("[data-read-age]")?.textContent ?? "";
    expect(age).toContain(READ_ISO);
    expect(age).toContain(NEWEST_ISO);

    const text = drawn(container);
    expect(text).not.toMatch(/\b(score|rank|winner|total|top player|best)\b/i);
  });

  it("an empty board is a sentence with a reason, never a table of zeroes", async () => {
    serve({ "GET /v1/board": { body: { board: [], read_at: READ_AT, newest_at: null } } });
    const { container } = renderAt("/board");
    expect(await screen.findByText(/no settled calls yet/i)).toBeTruthy();
    expect(container.querySelector("table")).toBeNull();
    expect(container.querySelector("[data-read-age]")?.textContent).toContain(READ_ISO);
  });

  it("an unreachable server is not drawn as an empty board", async () => {
    vi.stubGlobal("fetch", vi.fn(() => Promise.reject(new Error("down"))));
    const { container } = renderAt("/board");
    expect(await screen.findByText(/board could not be read/i)).toBeTruthy();
    expect(container.querySelector("table")).toBeNull();
    expect(drawn(container)).not.toMatch(/no settled calls yet/i);
  });

  it("a closed round shows how many calls were filed and how it settled, never the split", async () => {
    const seen = serve({
      ...SIGNED_OUT,
      "GET /v1/rounds/r1": CLOSED_ROUND,
      "GET /v1/rounds/r1/forecasts": {
        body: {
          round: "r1",
          closed: true,
          forecasts: [
            { ...COIN_A, side: "rug" },
            { ...COIN_A, side: "rug" },
            { ...COIN_A, side: "real" },
          ],
          read_at: READ_AT,
          newest_at: NEWEST_AT,
        },
      },
      "GET /v1/rounds/r1/outcomes": {
        body: {
          round: "r1",
          closed: true,
          outcomes: [{ ...COIN_A, reading: "Rugged", settled_at: NEWEST_AT }],
          read_at: READ_AT,
          newest_at: NEWEST_AT,
        },
      },
    });
    const { container } = renderAt("/play/r1");
    expect(await screen.findByText(/Calls filed on this coin: n = 3/)).toBeTruthy();
    const text = drawn(container);
    expect(text).toContain("Rugged");
    expect(text).toContain(READ_ISO);
    expect(text).toContain(NEWEST_ISO);
    // Q2: no crowd signal. Neither a share nor a per-side count.
    expect(text).not.toMatch(/\d+\s*%|\bcalled (it )?(real|rug)\b.*\d|2 (of|called)|\brug: \d/i);
    expect(seen.some((s) => s.url.pathname.endsWith("/forecasts"))).toBe(true);
  });

  it("does not draw a forecast list from a response that says the round is still open", async () => {
    serve({
      ...SIGNED_OUT,
      "GET /v1/rounds/r1": CLOSED_ROUND,
      "GET /v1/rounds/r1/forecasts": {
        body: {
          round: "r1",
          closed: false,
          forecasts: [{ ...COIN_A, side: "rug" }],
          read_at: READ_AT,
          newest_at: NEWEST_AT,
        },
      },
      "GET /v1/rounds/r1/outcomes": {
        body: { round: "r1", closed: false, read_at: READ_AT, newest_at: null },
      },
    });
    const { container } = renderAt("/play/r1");
    await waitFor(() => expect(drawn(container)).toMatch(/window closed/i));
    expect(drawn(container)).not.toMatch(/Calls filed on this coin/);
  });
});

describe("an open round", () => {
  it("shows the coins, the close time and the age of the reading, and no other player's call", async () => {
    // The adversarial server: it would answer the hidden routes if asked.
    const seen = serve({
      ...SIGNED_OUT,
      "GET /v1/rounds/r1": OPEN_ROUND,
      "GET /v1/rounds/r1/forecasts": {
        body: {
          round: "r1",
          closed: true,
          forecasts: [
            { ...COIN_A, side: "rug" },
            { ...COIN_B, side: "real" },
          ],
          read_at: READ_AT,
          newest_at: NEWEST_AT,
        },
      },
      "GET /v1/rounds/r1/outcomes": {
        body: {
          round: "r1",
          closed: true,
          outcomes: [{ ...COIN_A, reading: "Rugged", settled_at: NEWEST_AT }],
          read_at: READ_AT,
          newest_at: NEWEST_AT,
        },
      },
    });
    const { container } = renderAt("/play/r1");
    expect(await screen.findByText(COIN_A.token)).toBeTruthy();
    expect(screen.getByText(COIN_B.token)).toBeTruthy();

    const text = drawn(container);
    expect(text).toContain(new Date(CLOSE_AT * 1000).toISOString());
    expect(container.querySelector("[data-read-age]")?.textContent).toContain(READ_ISO);

    // It never asked for what the server hides ...
    const asked = seen.map((s) => s.url.pathname);
    expect(asked).not.toContain("/v1/rounds/r1/forecasts");
    expect(asked).not.toContain("/v1/rounds/r1/outcomes");
    // ... and it does not imply it: no count, not even a zero, no "so far".
    expect(text).not.toMatch(/Calls filed|\bn = \d|so far|be the first|no calls|nobody has|others have|\d+ calls?\b/i);
    expect(text).not.toContain("Rugged");
  });

  it("shows the signed-in player their own call, and only theirs", async () => {
    serve({
      ...SIGNED_IN,
      "GET /v1/rounds/r1": OPEN_ROUND,
      "GET /forecast/mine": {
        body: {
          round: "r1",
          board_id: "p-mine",
          forecasts: [{ ...COIN_A, side: "rug", submitted_at: NEWEST_AT, window_close: CLOSE_AT }],
          read_at: READ_AT,
        },
      },
    });
    const { container } = renderAt("/play/r1");
    expect(await screen.findByText(/Your call: rug/)).toBeTruthy();
    // The coin already called has no form; the other still does.
    expect(screen.queryByRole("button", { name: `Call ${COIN_A.token} real` })).toBeNull();
    expect(screen.getByRole("button", { name: `Call ${COIN_B.token} real` })).toBeTruthy();
    expect(drawn(container)).not.toMatch(/Calls filed|\bn = \d/);
  });
});

describe("the form that files a call", () => {
  it("sends the session cookie and the CSRF header, with the four fields and nothing else", async () => {
    const seen = serve({
      ...SIGNED_IN,
      "GET /v1/rounds/r1": OPEN_ROUND,
      "GET /forecast/mine": {
        body: { round: "r1", board_id: "p-mine", forecasts: [], read_at: READ_AT },
      },
      "POST /forecast": { status: 201, body: { saved: true } },
    });
    renderAt("/play/r1");
    const button = await screen.findByRole("button", { name: `Call ${COIN_B.token} a rug` });
    fireEvent.click(button);

    await waitFor(() => expect(seen.some((s) => s.init?.method === "POST")).toBe(true));
    const post = seen.find((s) => s.init?.method === "POST");
    expect(post?.url.origin).toBe(API);
    expect(post?.url.pathname).toBe("/forecast");
    expect(post?.init?.credentials).toBe("include");
    const headers = new Headers(post?.init?.headers as HeadersInit);
    expect(headers.get(CSRF_HEADER)).toBe("csrf-abc");
    expect(headers.get("content-type")).toBe("application/json");
    expect(JSON.parse(String(post?.init?.body))).toEqual({
      round: "r1",
      chain: COIN_B.chain,
      token: COIN_B.token,
      side: "rug",
    });
  });

  it("says the first call stands when the server refuses a second", async () => {
    serve({
      ...SIGNED_IN,
      "GET /v1/rounds/r1": OPEN_ROUND,
      "GET /forecast/mine": {
        body: { round: "r1", board_id: "p-mine", forecasts: [], read_at: READ_AT },
      },
      "POST /forecast": { status: 409, body: { error: "already called" } },
    });
    renderAt("/play/r1");
    fireEvent.click(await screen.findByRole("button", { name: `Call ${COIN_A.token} real` }));
    expect(await screen.findByText(/first call stands/i)).toBeTruthy();
  });

  it("is off when it cannot tell whether you are signed in", async () => {
    serve({
      "GET /auth/me": { status: 500, body: { error: "boom" } },
      "GET /v1/rounds/r1": OPEN_ROUND,
    });
    renderAt("/play/r1");
    expect(await screen.findByText(COIN_A.token)).toBeTruthy();
    await screen.findByText(/could not tell whether you are signed in/i);
    expect(screen.queryByRole("button", { name: /^Call /i })).toBeNull();
  });

  it("is not offered to someone signed out, who is sent to sign in with X", async () => {
    serve({ ...SIGNED_OUT, "GET /v1/rounds/r1": OPEN_ROUND });
    renderAt("/play/r1");
    const link = await screen.findByRole("link", { name: /sign in with x/i });
    expect(link.getAttribute("href")).toBe(`${API}/auth/x/start`);
    expect(screen.queryByRole("button", { name: /^Call /i })).toBeNull();
  });

  it("signs out with the CSRF header and the cookie", async () => {
    const seen = serve({
      ...SIGNED_IN,
      "GET /v1/rounds/r1": OPEN_ROUND,
      "GET /forecast/mine": {
        body: { round: "r1", board_id: "p-mine", forecasts: [], read_at: READ_AT },
      },
      "POST /auth/logout": { body: { signed_out: true } },
    });
    renderAt("/play/r1");
    fireEvent.click(await screen.findByRole("button", { name: /sign out/i }));
    await waitFor(() => expect(seen.some((s) => s.url.pathname === "/auth/logout")).toBe(true));
    const out = seen.find((s) => s.url.pathname === "/auth/logout");
    expect(out?.init?.method).toBe("POST");
    expect(out?.init?.credentials).toBe("include");
    expect(new Headers(out?.init?.headers as HeadersInit).get(CSRF_HEADER)).toBe("csrf-abc");
  });

  it("shows sign-in as not open on a 503 from /auth/me, with no link to the API", async () => {
    serve({
      "GET /auth/me": { status: 503, body: { error: "sign-in is not configured" } },
      "GET /v1/rounds/r1": OPEN_ROUND,
    });
    const { container } = renderAt("/play/r1");
    await screen.findByText(COIN_A.token);
    await waitFor(() => expect(drawn(container)).toContain("Sign-in is not open yet"));
    expect(container.querySelector(`a[href="${API}/auth/x/start"]`)).toBeNull();
    expect(screen.queryByRole("button", { name: /^Call /i })).toBeNull();
  });

  it("public reads carry no cookie", async () => {
    const seen = serve({ ...SIGNED_OUT, "GET /v1/rounds/r1": OPEN_ROUND });
    renderAt("/play/r1");
    await screen.findByText(COIN_A.token);
    const round = seen.find((s) => s.url.pathname === "/v1/rounds/r1");
    expect(round?.init?.credentials).toBe("omit");
  });
});

describe("my calls and the privacy notice", () => {
  const MINE = {
    body: { round: "r1", board_id: "p-mine", forecasts: [], read_at: READ_AT },
  };

  it("deletes the account only after a confirm step, with the CSRF header and the cookie", async () => {
    const table: Record<string, Canned> = {
      ...SIGNED_IN,
      "GET /forecast/mine": MINE,
      get "POST /account/delete"(): Canned {
        // The server has removed the identity row: the next /auth/me is 401.
        table["GET /auth/me"] = SIGNED_OUT["GET /auth/me"] as Canned;
        return { body: { deleted: true } };
      },
    };
    const seen = serve(table);
    const { container } = renderAt("/my-calls/r1");
    fireEvent.click(await screen.findByRole("button", { name: "Delete my account" }));
    // The confirm step says what goes and what stays, and has sent nothing yet.
    const text = drawn(container);
    expect(text).toContain("your X id, your handle, your account creation date and your session");
    expect(text).toContain("append-only public record");
    expect(text).toContain("nothing in the store links that key to your X account");
    expect(gameCopyViolations(text)).toEqual([]);
    expect(seen.some((s) => s.url.pathname === "/account/delete")).toBe(false);

    fireEvent.click(screen.getByRole("button", { name: /yes, delete my account/i }));
    await waitFor(() => expect(seen.some((s) => s.url.pathname === "/account/delete")).toBe(true));
    const del = seen.find((s) => s.url.pathname === "/account/delete");
    expect(del?.init?.method).toBe("POST");
    expect(del?.init?.credentials).toBe("include");
    expect(new Headers(del?.init?.headers as HeadersInit).get(CSRF_HEADER)).toBe("csrf-abc");
    // Then the signed-out state, and the control is gone.
    expect(await screen.findByText(/your account was deleted/i)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Delete my account" })).toBeNull();
    expect(container.querySelector(`a[href="${API}/auth/x/start"]`)).not.toBeNull();
  });

  it("keeps the account when the player backs out of the confirm step", async () => {
    const seen = serve({ ...SIGNED_IN, "GET /forecast/mine": MINE });
    renderAt("/my-calls/r1");
    fireEvent.click(await screen.findByRole("button", { name: "Delete my account" }));
    fireEvent.click(screen.getByRole("button", { name: /keep my account/i }));
    expect(screen.getByRole("button", { name: "Delete my account" })).toBeTruthy();
    expect(seen.some((s) => s.url.pathname === "/account/delete")).toBe(false);
  });

  it("offers no delete control when signed out", async () => {
    serve(SIGNED_OUT);
    renderAt("/my-calls/r1");
    await screen.findByText(/sign in to see your calls/i);
    expect(screen.queryByRole("button", { name: /delete my account/i })).toBeNull();
  });

  it("says nothing was deleted when the server refuses", async () => {
    serve({
      ...SIGNED_IN,
      "GET /forecast/mine": MINE,
      "POST /account/delete": { status: 403, body: { error: "csrf token missing or wrong" } },
    });
    const { container } = renderAt("/my-calls/r1");
    fireEvent.click(await screen.findByRole("button", { name: "Delete my account" }));
    fireEvent.click(screen.getByRole("button", { name: /yes, delete my account/i }));
    expect(await screen.findByRole("alert")).toBeTruthy();
    expect(drawn(container)).not.toContain("was deleted");
  });

  it("lists only the signed-in player's own calls with their board id and read age", async () => {
    const seen = serve({
      ...SIGNED_IN,
      "GET /forecast/mine": {
        body: {
          round: "r1",
          board_id: "p-mine",
          forecasts: [{ ...COIN_A, side: "real", submitted_at: NEWEST_AT, window_close: CLOSE_AT }],
          read_at: READ_AT,
        },
      },
    });
    const { container } = renderAt("/my-calls/r1");
    expect(await screen.findByText("p-mine")).toBeTruthy();
    expect(drawn(container)).toContain("You called it real");
    expect(container.querySelector("[data-read-age]")?.textContent).toContain(READ_ISO);
    const mine = seen.find((s) => s.url.pathname === "/forecast/mine");
    expect(mine?.url.searchParams.get("round")).toBe("r1");
    expect(mine?.init?.credentials).toBe("include");
  });

  it("asks nothing of the calls route when signed out", async () => {
    const seen = serve(SIGNED_OUT);
    renderAt("/my-calls/r1");
    expect(await screen.findByText(/sign in to see your calls/i)).toBeTruthy();
    expect(seen.some((s) => s.url.pathname === "/forecast/mine")).toBe(false);
  });

  it("renders the privacy notice as the server returned it", async () => {
    serve({
      "GET /v1/privacy": {
        body: {
          kept: [{ field: "X handle", why: "shown to you in the app", ends: "deleted by account deletion" }],
          not_kept: ["any wallet address: sign-in is X only"],
          sessions: "You have one session at a time.",
          deletion: "POST /account/delete removes your identity record.",
          public: "After a round closes its forecasts are public without any player named.",
        },
      },
    });
    const { container } = renderAt("/game-privacy");
    expect(await screen.findByText(/any wallet address: sign-in is X only/)).toBeTruthy();
    const text = drawn(container);
    expect(text).toContain("X handle");
    expect(text).toContain("You have one session at a time.");
    expect(text).toContain("POST /account/delete removes your identity record.");
  });

  it("says nothing of its own when the notice cannot be read", async () => {
    vi.stubGlobal("fetch", vi.fn(() => Promise.reject(new Error("down"))));
    const { container } = renderAt("/game-privacy");
    expect(await screen.findByText(/privacy notice could not be read/i)).toBeTruthy();
    expect(drawn(container)).not.toMatch(/what is kept/i);
  });
});

describe("the copy", () => {
  it("passes honesty.ts on every page, with and without a server", async () => {
    const pages = ["/play", "/play/r1", "/board", "/my-calls/r1"];
    const withServer = {
      ...SIGNED_IN,
      "GET /v1/rounds/r1": CLOSED_ROUND,
      "GET /v1/rounds/r1/forecasts": {
        body: { round: "r1", closed: true, forecasts: [{ ...COIN_A, side: "rug" }], read_at: READ_AT, newest_at: NEWEST_AT },
      },
      "GET /v1/rounds/r1/outcomes": {
        body: { round: "r1", closed: true, outcomes: [], read_at: READ_AT, newest_at: null },
      },
      "GET /v1/board": {
        body: { board: [{ board_id: "p-1", hits: 1, misses: 0, n: 1 }], read_at: READ_AT, newest_at: NEWEST_AT },
      },
      "GET /forecast/mine": {
        body: { round: "r1", board_id: "p-mine", forecasts: [], read_at: READ_AT },
      },
    };
    for (const configured of [true, false]) {
      for (const path of pages) {
        vi.stubEnv("VITE_API_BASE", configured ? API : "");
        serve(configured ? withServer : {});
        const { container, unmount } = renderAt(path);
        await waitFor(() => expect(drawn(container).length).toBeGreaterThan(200));
        // Let the reads land before reading the text.
        await new Promise((r) => setTimeout(r, 20));
        expect(gameCopyViolations(drawn(container)), `${path} (server: ${configured})`).toEqual([]);
        unmount();
      }
    }
  });

  it("says plainly that it is free, has no prize, is research, and treats its own token like any other", async () => {
    serve({ ...SIGNED_OUT, "GET /v1/rounds/r1": OPEN_ROUND });
    const { container } = renderAt("/play/r1");
    await screen.findByText(COIN_A.token);
    const text = drawn(container);
    expect(text).toMatch(/free to play, no prize/i);
    expect(text).toMatch(/piece of research/i);
    expect(text).toMatch(/not advice/i);
    expect(text).toMatch(/project's own token is treated exactly like any other/i);
  });

  it("the checker refuses the words the pages must never use", () => {
    for (const bad of ["Winner of the round", "earn rewards", "a prize for the best", "holders get more", "the leaderboard", "airdrop"]) {
      expect(gameCopyViolations(bad), bad).not.toEqual([]);
    }
    // ... and lets the call window and the denial through.
    expect(gameCopyViolations("The window closes soon. Free to play, no prize.")).toEqual([]);
  });
});
