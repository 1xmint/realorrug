// SPDX-License-Identifier: Apache-2.0
//! A server that answers with headers and then stalls must not leave a page on
//! "Reading…". `get()` used to clear its abort timer as soon as `fetch`
//! resolved, so a body that never finished was never aborted and `pool()` never
//! settled. The fake `fetch` here behaves like a browser's: it resolves with
//! headers, and its `json()` rejects only when the signal aborts. Re-apply the
//! bug (move `clearTimeout` back above `response.json()`) and the promise never
//! settles, so the test fails.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { pool } from "./api";

function withStalledBody() {
  vi.stubGlobal(
    "fetch",
    vi.fn((_url: string, init?: RequestInit) =>
      Promise.resolve({
        ok: true,
        json: () =>
          new Promise((_resolve, reject) => {
            init?.signal?.addEventListener("abort", () =>
              reject(new DOMException("aborted", "AbortError")),
            );
          }),
      } as Response),
    ),
  );
}

beforeEach(() => {
  vi.useFakeTimers();
  withStalledBody();
});
afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("a stalled response body", () => {
  it("gives up at the timeout instead of hanging", async () => {
    let settled = false;
    const result = pool().then((value) => {
      settled = true;
      return value;
    });

    await vi.advanceTimersByTimeAsync(3999);
    expect(settled).toBe(false);

    await vi.advanceTimersByTimeAsync(1);
    expect(settled).toBe(true);
    expect((await result).vault).toBeNull();
  });
});
