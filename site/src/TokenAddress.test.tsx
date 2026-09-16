// SPDX-License-Identifier: Apache-2.0
//! The three states `TokenAddress` can be in, and nothing in between.
//!
//! Unset and malformed must land on the *same* refusal, not two different
//! ones -- AGENTS.md rule 7, deny by default when config is missing. The one
//! way this component fails badly is printing a string that looks like a
//! contract address when it should not, so that is what every case here
//! checks for, not just the happy path.

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { TokenAddress } from "./TokenAddress";

afterEach(() => {
  cleanup();
  vi.unstubAllEnvs();
});

const VALID = "0x22fd1234567890abcdef1234567890abcdef48fa";

describe("no address configured", () => {
  it("says the token has not launched, and prints no 0x string", () => {
    render(<TokenAddress />);
    expect(screen.getByText(/has not launched/i)).toBeTruthy();
    expect(screen.getByText(/did not come from here/i)).toBeTruthy();
    const { container } = render(<TokenAddress />);
    expect(container.textContent ?? "").not.toMatch(/0x[0-9a-fA-F]{40}/);
  });
});

describe("an address configured but malformed", () => {
  it("is treated exactly like unset -- deny by default", () => {
    // Too short, and missing the `0x` that makes something worth trusting.
    vi.stubEnv("VITE_TOKEN_ADDRESS", "not-an-address");
    render(<TokenAddress />);
    expect(screen.getByText(/has not launched/i)).toBeTruthy();
    const { container } = render(<TokenAddress />);
    expect(container.textContent ?? "").not.toMatch(/not-an-address/);
  });

  it("also refuses a string one character short of shaped", () => {
    vi.stubEnv("VITE_TOKEN_ADDRESS", VALID.slice(0, -1));
    render(<TokenAddress />);
    expect(screen.getByText(/has not launched/i)).toBeTruthy();
  });
});

describe("a real address configured", () => {
  it("renders the address and a copy button, and says nothing about launching", () => {
    vi.stubEnv("VITE_TOKEN_ADDRESS", VALID);
    render(<TokenAddress />);
    expect(screen.getByText(VALID)).toBeTruthy();
    expect(
      screen.getByRole("button", { name: /copy/i }),
    ).toBeTruthy();
    expect(screen.queryByText(/has not launched/i)).toBeNull();
  });

  it("trims whitespace around a configured address before showing it", () => {
    vi.stubEnv("VITE_TOKEN_ADDRESS", `  ${VALID}  `);
    render(<TokenAddress />);
    expect(screen.getByText(VALID)).toBeTruthy();
  });
});
