// SPDX-License-Identifier: Apache-2.0
//! The parts every page of the forecasting game shares (design 0032 §13).
//!
//! # What these components refuse to draw
//!
//! - **No mock data.** With no API base configured [`GameOff`] is the whole
//!   page. There is no sample round, no example board and no "demo" mode,
//!   because a sample drawn beside the real thing is read as the real thing.
//! - **No figure without its age.** [`ReadAge`] is how a public figure gets its
//!   `read_at` and `newest_at` (ADR 0039 decision 6), and the pages that show a
//!   count show it through this component or not at all.
//! - **No prize, no value, no advice.** [`GameNotice`] says so on every page in
//!   plain words, and `game.test.tsx` runs every page's rendered text through
//!   `gameCopyViolations`.

import { useCallback, useEffect, useState } from "react";

import { gameHref, isoOf, me, signOut } from "./game";
import { measuredAgo } from "./honesty";
import { Card, Here, Nothing } from "./ui";

/** What the game is, in the words the owner decided (design 0032 Q1 to Q3). */
export function GameNotice() {
  return (
    <Card className="mt-10 max-w-2xl">
      <p className="text-sm text-[var(--color-dim)]">
        <strong className="text-[var(--color-text)]">Free to play, no prize.</strong>{" "}
        Nothing of value is paid to anyone for any result, and nothing here asks
        for a wallet address. A call is a piece of research about a token&apos;s
        launch, not advice to buy, sell or hold anything. The project&apos;s own
        token is treated exactly like any other. See what is kept about you in
        the <Here href="/game-privacy">privacy notice</Here>.
      </p>
    </Card>
  );
}

/** The page's whole body when the game has no server to talk to. */
export function GameOff() {
  return (
    <Nothing
      what="The forecasting game is not running."
      why="This site has no game server configured, so there are no rounds, no board and no sign-in to show. Nothing on this page is sample data, and a missing server is not the same as an empty round."
    />
  );
}

/** A server that did not answer, or answered with a refusal. Says which. */
export function GameProblem({
  what,
  status,
  error,
}: {
  what: string;
  status?: number;
  error?: string | null;
}) {
  return (
    <Nothing
      what={what}
      why={
        error
          ? `The server said: ${error}${status ? ` (${status})` : ""}.`
          : "The server could not be reached. This says nothing about any round or any call; try again in a minute."
      }
    />
  );
}

/**
 * How old a public reading is, and how big.
 *
 * `n` is the sample size where the figure is an aggregate. A page that shows a
 * count passes it; a page that shows no count passes none, and the line then
 * says nothing about size rather than "0".
 */
export function ReadAge({
  readAt,
  newestAt,
  n,
  newestLabel = "Newest row this draws on",
  whenEmpty,
}: {
  readAt: number;
  newestAt: number | null;
  n?: number;
  newestLabel?: string;
  /** Said when there is no newest row. Omitted on an open round, where "no row" would imply a count. */
  whenEmpty?: string;
}) {
  const read = isoOf(readAt);
  const ago = read === null ? null : measuredAgo(read);
  const newest = isoOf(newestAt);
  return (
    <p className="mt-6 text-xs text-[var(--color-faint)]" data-read-age>
      Read {ago ?? "at an unknown time"}
      {read ? ` (${read})` : ""}.{" "}
      {newest ? `${newestLabel}: ${newest}. ` : whenEmpty ? `${whenEmpty} ` : ""}
      {n !== undefined ? `Sample size n = ${n}. ` : ""}
      Every figure here is a reading at that moment and can be up to 30 seconds
      older than the server.
    </p>
  );
}

export type Session =
  | { readonly kind: "loading" }
  | { readonly kind: "out" }
  | { readonly kind: "in"; readonly handle: string; readonly csrf: string }
  | { readonly kind: "unknown" };

/**
 * Who, if anyone, is signed in.
 *
 * A 401 is signed out. Anything else that is not an answer is `unknown`, which
 * turns the call form off: unknown is not signed-out and not signed-in (rule
 * 8), and a form that posts on a guess would file a call for the wrong state.
 */
export function useSession(): {
  session: Session;
  reload: () => void;
} {
  const [session, setSession] = useState<Session>({ kind: "loading" });
  const [tick, setTick] = useState(0);
  useEffect(() => {
    let live = true;
    void me().then((reply) => {
      if (!live) return;
      if (reply.kind === "ok" && reply.body.signed_in === true) {
        setSession({ kind: "in", handle: reply.body.handle, csrf: reply.body.csrf_token });
      } else if (reply.kind === "refused" && reply.status === 401) {
        setSession({ kind: "out" });
      } else {
        setSession({ kind: "unknown" });
      }
    });
    return () => {
      live = false;
    };
  }, [tick]);
  const reload = useCallback(() => setTick((t) => t + 1), []);
  return { session, reload };
}

/** Sign in with X, or who you are and a way out. */
export function SessionBar({
  session,
  onChange,
}: {
  session: Session;
  onChange: () => void;
}) {
  const [problem, setProblem] = useState<string | null>(null);
  const start = gameHref("/auth/x/start");

  if (session.kind === "loading") {
    return <p className="text-sm text-[var(--color-faint)]">Checking sign-in…</p>;
  }
  if (session.kind === "unknown") {
    return (
      <p className="text-sm text-[var(--color-dim)]">
        Could not tell whether you are signed in, so calls are off until this
        loads. Reload the page to try again.
      </p>
    );
  }
  if (session.kind === "out") {
    return (
      <p className="text-sm text-[var(--color-dim)]">
        Not signed in.{" "}
        {start !== null && (
          <a
            href={start}
            className="text-[var(--color-signal)] underline underline-offset-4 hover:text-[var(--color-text)]"
          >
            Sign in with X
          </a>
        )}{" "}
        to file a call. Signing in reads your handle once; X sign-in is the only
        way in.
      </p>
    );
  }
  const out = async () => {
    setProblem(null);
    const reply = await signOut(session.csrf);
    if (reply.kind === "ok") onChange();
    else setProblem("Sign-out did not go through. You are still signed in.");
  };
  return (
    <div className="flex flex-wrap items-center gap-3 text-sm text-[var(--color-dim)]">
      <span>Signed in as @{session.handle}.</span>
      <button
        type="button"
        onClick={() => void out()}
        className="text-[var(--color-signal)] underline underline-offset-4 hover:text-[var(--color-text)]"
      >
        Sign out
      </button>
      {problem !== null && <span role="alert">{problem}</span>}
    </div>
  );
}
