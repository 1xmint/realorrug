// SPDX-License-Identifier: Apache-2.0
//! The home page's live feed, with a server that has posted verdicts.

import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { Home, shortAddress } from "./Home";

const TOKEN = "0x22fd486d80b7cce7362ffed59bbf2fd266a148fa";

function withRecent(verdicts: unknown[]) {
  vi.stubGlobal(
    "fetch",
    vi.fn((url: string) =>
      String(url).endsWith("/v1/public/recent")
        ? Promise.resolve(
            new Response(JSON.stringify({ measured_at: null, verdicts }), {
              status: 200,
            }),
          )
        : Promise.reject(new Error("no server")),
    ),
  );
}

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("the live feed", () => {
  it("shows each posted verdict's stamp, linked to its own check", async () => {
    withRecent([
      {
        address: TOKEN,
        chain: "robinhood",
        level: "RugMechanicsLive",
        at: new Date().toISOString(),
        reply_url: "https://x.com/i/web/status/123",
      },
    ]);
    render(<Home />);
    await waitFor(() => {
      expect(screen.getByText("Rug mechanics live")).toBeTruthy();
    });
    const link = screen.getByText(shortAddress(TOKEN)).closest("a");
    expect(link?.getAttribute("href")).toBe(`/check/${TOKEN}`);
    const reply = screen.getByText("the reply").closest("a");
    expect(reply?.getAttribute("href")).toBe("https://x.com/i/web/status/123");
  });

  it("drops a reply link that does not point at x.com", async () => {
    withRecent([
      {
        address: TOKEN,
        chain: "robinhood",
        level: "Sketchy",
        at: new Date().toISOString(),
        reply_url: "https://evil.example/#@x.com",
      },
    ]);
    render(<Home />);
    await waitFor(() => {
      expect(screen.getByText(shortAddress(TOKEN))).toBeTruthy();
    });
    expect(screen.queryByText("the reply")).toBeNull();
  });

  it("says nothing has been posted when the list is empty", async () => {
    withRecent([]);
    render(<Home />);
    await waitFor(() => {
      expect(screen.getByText(/No verdicts posted yet/i)).toBeTruthy();
    });
  });

  it("shortens an address to its two ends", () => {
    expect(shortAddress(TOKEN)).toBe("0x22fd…48fa");
    expect(shortAddress("short")).toBe("short");
  });
});
