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

import { deleteAccount, gameHref, isoOf, me, signOut } from "./game";
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
  | { readonly kind: "closed" }
  | { readonly kind: "in"; readonly handle: string; readonly csrf: string }
  | { readonly kind: "unknown" };

/**
 * Who, if anyone, is signed in.
 *
 * A 401 is signed out. A 503 is "sign-in is not open", which is neither out nor
 * unknown: the server said so. The one closed state this cannot see is an
 * exhausted allowance for reading X accounts, which only `/auth/x/start`
 * reports, so the link below says what to expect.
 * Anything else that is not an answer is `unknown`, which
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
      } else if (reply.kind === "refused" && reply.status === 503) {
        // The server's own words for "sign-in is not configured": every
        // session route answers 503 then, and /auth/me is how the page can
        // know before it offers a link that would land on the API's JSON.
        setSession({ kind: "closed" });
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
  if (session.kind === "closed") {
    return (
      <p className="text-sm text-[var(--color-dim)]">
        Sign-in is not open yet, so calls cannot be filed. Nothing is wrong with
        your browser; the public pages still work.
      </p>
    );
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
        to file a call. Signing in reads your handle, your X id and your account
        creation date once; X sign-in is the only way in. If sign-in is closed when you follow the link, the server shows a
        short error and nothing is signed in; come back later.
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

/**
 * "Delete my account", behind a confirm step that says what goes and what stays.
 *
 * The words are the server's own (`/v1/privacy`, design 0032 §4): the identity
 * record goes, and the forecast rows, which are an append-only public record,
 * stay under a random key with nothing left that links it to an X account.
 * Signed-in only, and the POST carries the session cookie and the CSRF token
 * like every other POST. Nothing is sent until the second button.
 */
export function DeleteAccount({
  csrf,
  onDeleted,
}: {
  csrf: string;
  onDeleted: () => void;
}) {
  const [asking, setAsking] = useState(false);
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);

  const confirm = async () => {
    setBusy(true);
    setProblem(null);
    const reply = await deleteAccount(csrf);
    setBusy(false);
    if (reply.kind === "ok") onDeleted();
    else if (reply.kind === "refused") {
      setProblem(reply.error ?? `The server refused (${reply.status}). Nothing was deleted.`);
    } else {
      setProblem("The server could not be reached. Check whether you are still signed in before trying again.");
    }
  };

  if (!asking) {
    return (
      <p className="text-sm">
        <button
          type="button"
          onClick={() => setAsking(true)}
          className="text-[var(--color-dim)] underline underline-offset-4 hover:text-[var(--color-text)]"
        >
          Delete my account
        </button>
      </p>
    );
  }
  return (
    <Card>
      <p className="text-[var(--color-text)]">Delete your account?</p>
      <p className="mt-2 text-sm text-[var(--color-dim)]">
        <strong className="text-[var(--color-text)]">What is deleted:</strong> your identity record, which is
        your X id, your handle, your account creation date and your session. You are signed out.
      </p>
      <p className="mt-2 text-sm text-[var(--color-dim)]">
        <strong className="text-[var(--color-text)]">What stays:</strong> forecast and outcome rows are an
        append-only public record and are not rewritten. They stay under a random key, and after deletion
        nothing in the store links that key to your X account.
      </p>
      <div className="mt-4 flex flex-wrap gap-3">
        <button
          type="button"
          disabled={busy}
          onClick={() => void confirm()}
          className="display bg-[var(--color-raised)] px-5 py-2 text-[var(--color-text)] disabled:opacity-40"
        >
          Yes, delete my account
        </button>
        <button
          type="button"
          disabled={busy}
          onClick={() => setAsking(false)}
          className="px-3 py-2 text-sm text-[var(--color-dim)] underline underline-offset-4"
        >
          Keep my account
        </button>
      </div>
      {problem !== null && (
        <p role="alert" className="mt-3 text-sm">
          {problem}
        </p>
      )}
    </Card>
  );
}
