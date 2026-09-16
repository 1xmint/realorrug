// SPDX-License-Identifier: Apache-2.0
//! The product's own token address, or the honest fact that it does not have
//! one yet.
//!
//! # Deny by default, because a guess here is worse than silence
//!
//! The token (ADR 0013, the one the operator holds none of) has not launched.
//! No address for it exists anywhere, which means every address a stranger
//! might paste into a search engine claiming to be "the Real or Rug token" is
//! either a scam contract or a coincidence, and this component's whole job is
//! to never be the thing that makes one of those look official.
//!
//! So the address only renders when `VITE_TOKEN_ADDRESS` is both set and
//! shaped like a Robinhood Chain contract address ([`evmShaped`]). Unset,
//! empty, or malformed all fall to the same refusal — AGENTS.md rule 7, "deny
//! by default when config is missing" — rather than three different ways of
//! almost showing something.

import { useState } from "react";

import { evmShaped } from "./honesty";

/** The configured address, only once it is shaped like one. `null` otherwise. */
function configuredAddress(): string | null {
  const configured = import.meta.env["VITE_TOKEN_ADDRESS"];
  if (typeof configured !== "string") return null;
  const trimmed = configured.trim();
  return evmShaped(trimmed) ? trimmed : null;
}

export function TokenAddress() {
  const address = configuredAddress();
  const [copied, setCopied] = useState(false);

  if (address === null) {
    return (
      <p className="mt-6 max-w-2xl text-sm text-[var(--color-dim)]">
        The token has not launched. Nobody has a contract address for it yet —
        an address claiming to be Real or Rug&apos;s token did not come from
        here.
      </p>
    );
  }

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(address);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // Clipboard permission denied, or no `navigator.clipboard` at all (an
      // insecure context, mainly). The address is still selectable text
      // beside the button, so a reader can copy it by hand either way.
    }
  };

  return (
    <div className="mt-6 flex flex-wrap items-center gap-3">
      <span className="typewriter text-sm text-[var(--color-dim)]">
        Contract
      </span>
      <code className="rounded bg-[var(--color-raised)] px-2 py-1 font-mono text-sm text-[var(--color-text)]">
        {address}
      </code>
      <button
        type="button"
        onClick={() => void copy()}
        className="text-sm text-[var(--color-signal)] underline underline-offset-4 hover:text-[var(--color-text)]"
      >
        {copied ? "Copied" : "Copy"}
      </button>
    </div>
  );
}
