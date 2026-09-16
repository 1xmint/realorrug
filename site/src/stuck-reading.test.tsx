// SPDX-License-Identifier: Apache-2.0
//! `Leaderboard` and `Pool` used to have exactly one branch for "the fetch
//! has not resolved yet", and nothing bounded how long that branch could
//! stay true. `api.ts`'s own `get()` catches a rejection and a timed-out
//! `AbortController`, but neither of those fires if the underlying `fetch`
//! never calls back at all -- a hung connection some environments do not
//! abort the way `TIMEOUT_MS` assumes. When that happens the promise
//! `leaderboard()`/`pool()` return never settles, `setData` is never called,
//! and the page reads "Reading…" forever with no way out and no explanation.
//!
//! This file reproduces that exact shape -- a `fetch` mock that never
//! resolves or rejects -- and asserts the page does not stay on "Reading…"
//! past its own watchdog: it lands on an error state that says what failed
//! and offers a retry. Re-apply the bug (delete either component's watchdog
//! `setTimeout`) and these fail by timing out on "Reading…" forever, the
//! same way the reader would.

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { Leaderboard } from "./Leaderboard";
import { Pool } from "./Pool";

/** A `fetch` that never resolves and never rejects -- the hang, not a failure. */
function withHungServer() {
  vi.stubGlobal(
    "fetch",
    vi.fn(() => new Promise<Response>(() => {})),
  );
}

// Real timers: the watchdog this file exists to prove is a real `setTimeout`,
// and faking React's own scheduler around it has proven unreliable across
// React/JSDOM versions. The watchdog is 8s, so these run a little slow
// rather than flaky -- `waitFor`'s own timeout is raised to match.
const PAST_WATCHDOG = 15000;

beforeEach(withHungServer);
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("the leaderboard, when the request never comes back", () => {
  it(
    "does not stay on Reading… forever",
    async () => {
      render(<Leaderboard />);
      expect(screen.getByText(/Reading…/)).toBeTruthy();

      // Past the component's own watchdog, with nothing from `fetch` at all.
      await waitFor(
        () => {
          expect(screen.getByText(/Could not reach the server/i)).toBeTruthy();
        },
        { timeout: PAST_WATCHDOG },
      );

      expect(screen.queryByText(/Reading…/)).toBeNull();
      // Says what failed, in words a stranger can act on, and does not claim
      // "no week has run" -- that is a different fact than "the request hung".
      expect(screen.queryByText(/No week has run yet/i)).toBeNull();
    },
    PAST_WATCHDOG + 5000,
  );

  it(
    "offers a retry that fires a new request",
    async () => {
      render(<Leaderboard />);
      await waitFor(
        () => {
          expect(screen.getByRole("button", { name: /try again/i })).toBeTruthy();
        },
        { timeout: PAST_WATCHDOG },
      );
      const fetchMock = fetch as unknown as ReturnType<typeof vi.fn>;
      const callsBeforeRetry = fetchMock.mock.calls.length;

      fireEvent.click(screen.getByRole("button", { name: /try again/i }));

      expect(fetchMock.mock.calls.length).toBeGreaterThan(callsBeforeRetry);
      // Back to a genuine loading state, not still showing the stale error.
      expect(screen.getByText(/Reading…/)).toBeTruthy();
    },
    PAST_WATCHDOG + 5000,
  );
});

describe("the pool, when the request never comes back", () => {
  it(
    "does not stay on Reading… forever",
    async () => {
      render(<Pool />);
      expect(screen.getByText(/Reading…/)).toBeTruthy();

      await waitFor(
        () => {
          expect(screen.getByText(/Could not reach the server/i)).toBeTruthy();
        },
        { timeout: PAST_WATCHDOG },
      );

      expect(screen.queryByText(/Reading…/)).toBeNull();
      // Never renders a balance -- 0 or otherwise -- for a request that hung.
      expect(screen.queryByText(/0\.0000\s*SOL/)).toBeNull();
      expect(screen.queryByText(/There is no token yet/i)).toBeNull();
    },
    PAST_WATCHDOG + 5000,
  );

  it(
    "offers a retry that fires a new request",
    async () => {
      render(<Pool />);
      await waitFor(
        () => {
          expect(screen.getByRole("button", { name: /try again/i })).toBeTruthy();
        },
        { timeout: PAST_WATCHDOG },
      );
      const fetchMock = fetch as unknown as ReturnType<typeof vi.fn>;
      const callsBeforeRetry = fetchMock.mock.calls.length;

      fireEvent.click(screen.getByRole("button", { name: /try again/i }));

      expect(fetchMock.mock.calls.length).toBeGreaterThan(callsBeforeRetry);
      expect(screen.getByText(/Reading…/)).toBeTruthy();
    },
    PAST_WATCHDOG + 5000,
  );
});
