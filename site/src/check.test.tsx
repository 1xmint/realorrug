// SPDX-License-Identifier: Apache-2.0
//! What the checker page says for every answer the server can give.
//!
//! The dangerous wrong versions all look fine: a "can't read" drawn green, an
//! unreachable server drawn as a verdict, a budget refusal that reads like a
//! finding about the token. Each test below is one of those, pinned.

import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { Router } from "wouter";
import { memoryLocation } from "wouter/memory-location";
import { afterEach, describe, expect, it, vi } from "vitest";

import { App } from "./App";
import { shareHref } from "./Check";
import { evmShaped } from "./honesty";

const ADDR = "0x22fd5e2c1a9b3d4e5f60718293a4b5c6d7e848fa";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

function serverSays(status: number, body: unknown) {
  const fetch = vi.fn(() =>
    Promise.resolve(new Response(JSON.stringify(body), { status })),
  );
  vi.stubGlobal("fetch", fetch);
  return fetch;
}

function body(over: Record<string, unknown>) {
  return {
    state: "verdict",
    chain: "robinhood",
    address: ADDR,
    level: null,
    reasons: [],
    twins: [],
    measured_at: null,
    message: null,
    ...over,
  };
}

function renderAt(path: string) {
  const { hook, history } = memoryLocation({ path, record: true });
  render(
    <Router hook={hook}>
      <App />
    </Router>,
  );
  return { history };
}

describe("the address shape", () => {
  it("accepts forty hex digits after 0x and nothing near it", () => {
    expect(evmShaped(ADDR)).toBe(true);
    expect(evmShaped(` ${ADDR} `)).toBe(true);
    expect(evmShaped(ADDR.slice(0, -1))).toBe(false);
    expect(evmShaped(`${ADDR}0`)).toBe(false);
    expect(evmShaped(ADDR.replace("0x", ""))).toBe(false);
    expect(evmShaped(`${ADDR.slice(0, -1)}g`)).toBe(false);
  });
});

describe("the checker page", () => {
  it("draws the verdict, the evidence and its innocent explanation", async () => {
    serverSays(
      200,
      body({
        level: "Sketchy",
        reasons: ["The creator holds 41% of supply."],
        twins: ["Some teams hold supply to fund work."],
        measured_at: "2026-09-16T12:00:00Z",
      }),
    );
    renderAt(`/check/${ADDR}`);
    await waitFor(() => expect(screen.getByText("Sketchy")).toBeTruthy());
    expect(screen.getByText(/creator holds 41%/)).toBeTruthy();
    expect(screen.getByText(/fund work/)).toBeTruthy();
    expect(screen.getByText(/2026-09-16T12:00:00Z/)).toBeTruthy();
  });

  it("never draws can't-read as clean", async () => {
    // Rule 8: unknown is not safe. The server sends no level with cant_read,
    // and the page must fall to the grey rung, not the green one.
    serverSays(200, body({ state: "cant_read" }));
    renderAt(`/check/${ADDR}`);
    await waitFor(() => expect(screen.getByText("Can't tell")).toBeTruthy());
    expect(screen.queryByText("Nothing ugly yet")).toBeNull();
  });

  it("draws a level it does not know as can't tell", async () => {
    serverSays(200, body({ level: "BrandNewLevel" }));
    renderAt(`/check/${ADDR}`);
    await waitFor(() => expect(screen.getByText("Can't tell")).toBeTruthy());
  });

  it("draws no verdict when the server cannot be reached", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(() => Promise.reject(new Error("no server"))),
    );
    renderAt(`/check/${ADDR}`);
    await waitFor(() =>
      expect(screen.getByText(/could not be reached/i)).toBeTruthy(),
    );
    expect(screen.getByText(/says nothing about the token/i)).toBeTruthy();
    for (const stamp of [
      "Rugged",
      "Sketchy",
      "Nothing ugly yet",
      "Can't tell",
    ]) {
      expect(screen.queryByText(stamp)).toBeNull();
    }
  });

  it.each([
    [429, "busy", /Too many checks/i],
    [503, "budget", /used today's reads/i],
    [200, "not_a_token", /no token at that address/i],
  ])("explains a %i %s in words", async (status, state, words) => {
    serverSays(status, body({ state }));
    renderAt(`/check/${ADDR}`);
    await waitFor(() => expect(screen.getByText(words)).toBeTruthy());
  });

  it("asks the server nothing for a string that is not an address", async () => {
    const fetch = serverSays(200, body({ level: "Rugged" }));
    renderAt("/check/not-an-address");
    expect(
      screen.getAllByText(/not a contract address/i).length,
    ).toBeGreaterThan(0);
    expect(fetch).not.toHaveBeenCalledWith(
      expect.stringContaining("/v1/check/"),
      expect.anything(),
    );
  });
});

describe("the paste box", () => {
  it("goes to the checker page for an address", () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(() => Promise.reject(new Error("no server"))),
    );
    const { history } = renderAt("/");
    fireEvent.change(screen.getByLabelText(/contract address/i), {
      target: { value: `  ${ADDR}  ` },
    });
    fireEvent.click(screen.getByRole("button", { name: /check it/i }));
    expect(history.at(-1)).toBe(`/check/${ADDR}`);
  });

  it("stays put and says why for something that is not one", () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(() => Promise.reject(new Error("no server"))),
    );
    const { history } = renderAt("/");
    fireEvent.change(screen.getByLabelText(/contract address/i), {
      target: { value: "pepe" },
    });
    fireEvent.click(screen.getByRole("button", { name: /check it/i }));
    expect(history.at(-1)).toBe("/");
    expect(screen.getByText(/not a contract address/i)).toBeTruthy();
  });

  it("takes a Solana mint to the checker too, not only an 0x one", () => {
    // The box has accepted both shapes since it was written, but until
    // 2026-09-17 it said "0x…" on the field and "should start with 0x and be
    // 42 characters long" when it refused, so a pump.fun holder read the front
    // door as closed. Re-apply the bug by dropping `mintShaped` from
    // `CheckBox`'s guard and this stays on "/".
    vi.stubGlobal(
      "fetch",
      vi.fn(() => Promise.reject(new Error("no server"))),
    );
    const mint = "So11111111111111111111111111111111111111112";
    const { history } = renderAt("/");
    fireEvent.change(screen.getByLabelText(/contract address/i), {
      target: { value: mint },
    });
    fireEvent.click(screen.getByRole("button", { name: /check it/i }));
    expect(history.at(-1)).toBe(`/check/${mint}`);
  });
});

describe("the share text", () => {
  it("names the level and links back to this page, and nothing else", () => {
    const href = shareHref("Sketchy", "Real red flags.", ADDR, "realorrug");
    const text = decodeURIComponent(href.split("text=")[1] ?? "");
    expect(text).toMatch(/^SKETCHY\. Real red flags\. Checked by @realorrug /);
    expect(text.endsWith(`/check/${ADDR}`)).toBe(true);
    expect(text).not.toMatch(/\$|price|market cap/i);
    const untagged = decodeURIComponent(
      shareHref("Sketchy", "Real red flags.", ADDR, null).split("text=")[1] ??
        "",
    );
    expect(untagged).not.toContain("@");
    expect(untagged).toMatch(/Checked on Real or Rug /);
  });
});
