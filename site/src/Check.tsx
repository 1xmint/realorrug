// SPDX-License-Identifier: Apache-2.0
//! The checker: one token's verdict, at an address anybody can share.
//!
//! Design 0023 is the page; design 0025 §4a is where it sits. The page reads
//! its address from the URL and asks the server, and the server -- not this
//! file -- decides the verdict, from code, with no model call. This file only
//! draws what came back, and every state the server can return has its own
//! words, because "something went wrong" on a verdict page reads as "rug".
//!
//! Two rules from AGENTS.md §3 bind the drawing:
//! - Rule 8: `CantTell` is drawn grey, never green, and an unreachable server
//!   is never drawn as a verdict at all.
//! - Rule 4: the share text (design 0023 §5) is composed here from the level
//!   alone. No free-typed caption, so a visitor cannot put words about a
//!   person next to the stamp.

import { useEffect, useState } from "react";
import { useRoute } from "wouter";

import { type CheckResult, check } from "./api";
import { evmShaped, mintShaped } from "./honesty";
import { LADDER } from "./HowItWorks";
import { useTitle } from "./title";
import { CheckBox, Heading, Nothing, Section } from "./ui";

/** Loading, a body from the server, or `null` when there was no usable body. */
type Seen = { readonly kind: "loading" } | { readonly kind: "done"; readonly result: CheckResult | null };

export function Check() {
  const [, params] = useRoute("/check/:address");
  const address = (params?.address ?? "").trim();
  const shaped = evmShaped(address) || mintShaped(address);
  const [seen, setSeen] = useState<Seen>({ kind: "loading" });

  useTitle("Check a token");

  useEffect(() => {
    // A shape that is not an address never costs the server a request: the
    // answer is already known, and the budget is for real reads.
    if (!shaped) return;
    let live = true;
    setSeen({ kind: "loading" });
    void check(address).then((result) => {
      if (live) setSeen({ kind: "done", result });
    });
    return () => {
      live = false;
    };
  }, [address, shaped]);

  return (
    <Section>
      <Heading kicker="Case file">Check a token</Heading>
      <p className="font-mono text-sm break-all text-[var(--color-dim)]">{address}</p>
      <div className="mt-10 max-w-2xl">
        {!shaped ? (
          <Refusal
            what="That is not a contract address."
            why="A Robinhood Chain address starts with 0x and is 42 characters long. Paste it again below."
          />
        ) : seen.kind === "loading" ? (
          <Reading />
        ) : (
          <Answer result={seen.result} address={address} />
        )}
      </div>
      <div className="mt-16">
        <CheckBox />
      </div>
    </Section>
  );
}

/** While the chain is being read. Cold reads take seconds, and the page says so. */
function Reading() {
  return (
    <div className="paper pinned p-8" role="status">
      <p className="typewriter text-lg">Reading the chain…</p>
      <p className="mt-3 text-sm text-[var(--color-paper-dim)]">
        A token nobody has asked about yet takes up to half a minute. The
        verdict is decided by code from what the chain shows, not guessed.
      </p>
    </div>
  );
}

function Refusal({ what, why }: { what: string; why: string }) {
  return <Nothing what={what} why={why} />;
}

function Answer({ result, address }: { result: CheckResult | null; address: string }) {
  if (result === null) {
    return (
      <Refusal
        what="The checker could not be reached."
        why="This says nothing about the token. Try again in a minute; if it keeps happening, the checker is down, not the token."
      />
    );
  }
  switch (result.state) {
    case "verdict":
      return <Verdict result={result} address={address} />;
    case "cant_read":
      return <Verdict result={{ ...result, level: "CantTell" }} address={address} />;
    case "not_a_token":
      return (
        <Refusal
          what="There is no token at that address."
          why={result.message ?? "The address exists, but nothing there behaves like a token. Check you copied the contract, not a wallet."}
        />
      );
    case "bad_address":
      return (
        <Refusal
          what="That is not a contract address."
          why={result.message ?? "Paste the token's contract address again below."}
        />
      );
    case "busy":
      return (
        <Refusal
          what="Too many checks from here in the last minute."
          why="Wait a minute and reload. The limit keeps the checker free for everybody."
        />
      );
    case "budget":
      return (
        <Refusal
          what="The checker has used today's reads."
          why="Tokens someone already checked today still load; a new one waits until the daily budget resets. This says nothing about the token."
        />
      );
    default:
      // A state added on the server before this page knows it. Unknown is not
      // safe (rule 8), so it is drawn as no answer rather than guessed at.
      return (
        <Refusal
          what="The checker gave an answer this page does not understand."
          why="Reload the page. If it persists, the site is older than the checker."
        />
      );
  }
}

function Verdict({ result, address }: { result: CheckResult; address: string }) {
  // No level, or a level this page does not know, is drawn as Can't tell:
  // the one rung that claims nothing.
  const rung = LADDER.find((l) => l.code === result.level) ?? LADDER[LADDER.length - 1]!;
  const share = shareHref(rung.stamp, rung.means, address);
  return (
    <article className="paper pinned p-6 pt-10 sm:p-10">
      <span className="stamp thump text-3xl sm:text-4xl" style={{ color: rung.ink }}>
        {rung.stamp}
      </span>
      <p className="typewriter mt-6 text-xl leading-snug">{rung.means}</p>
      <p className="mt-2 text-sm text-[var(--color-paper-dim)]">{rung.note}</p>

      {result.reasons.length > 0 && (
        <section className="mt-8 border-t border-dashed border-[var(--color-paper-dim)] pt-5">
          <h3 className="display text-lg">What the chain shows</h3>
          <ul className="mt-3 list-disc space-y-2 pl-5">
            {result.reasons.map((r) => (
              <li key={r}>{r}</li>
            ))}
          </ul>
        </section>
      )}

      {result.twins.length > 0 && (
        <section className="mt-6">
          <h3 className="display text-lg">The innocent explanation</h3>
          <ul className="mt-3 list-disc space-y-2 pl-5 text-[var(--color-paper-dim)]">
            {result.twins.map((t) => (
              <li key={t}>{t}</li>
            ))}
          </ul>
        </section>
      )}

      <div className="mt-8 flex flex-wrap items-center justify-between gap-4 border-t border-dashed border-[var(--color-paper-dim)] pt-5">
        <p className="text-xs text-[var(--color-paper-dim)]">
          {result.measured_at ? `Read from the chain at ${result.measured_at}. ` : ""}
          Not financial advice.
        </p>
        <a
          href={share}
          target="_blank"
          rel="noopener noreferrer"
          className="display bg-[var(--color-paper-ink)] px-5 py-2 text-base text-[var(--color-paper)] hover:opacity-85"
        >
          Share on X
        </a>
      </div>
    </article>
  );
}

/**
 * The share link, composed from the level and this page's own address.
 *
 * Exported for its test: the text is the rule-4 surface on this page.
 */
export function shareHref(stamp: string, means: string, address: string): string {
  const origin = typeof window === "undefined" ? "" : window.location.origin;
  const url = `${origin}/check/${address}`;
  const text = `Checked a token on Real or Rug: ${stamp}. ${means} ${url}`;
  return `https://x.com/intent/post?text=${encodeURIComponent(text)}`;
}
