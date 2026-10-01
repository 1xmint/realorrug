// SPDX-License-Identifier: Apache-2.0
//! The player's own calls in one round (`GET /forecast/mine?round=`).
//!
//! The route answers only for the session's own key and takes no parameter that
//! names somebody else, so there is nothing here that could show another
//! player's call. The reply is `no-store`; the page keeps it in memory only.
//! It also carries the player's opaque board id, so they can find their own
//! line on the board, which never shows a handle.

import { useEffect, useState } from "react";
import { useLocation, useRoute } from "wouter";

import { type Mine, type Reply, gameBase, isoOf, mine, roundIdShaped } from "./game";
import {
  DeleteAccount,
  GameNotice,
  GameOff,
  GameProblem,
  ReadAge,
  SessionBar,
  useSession,
} from "./GameUi";
import { useTitle } from "./title";
import { Card, Heading, Here, Nothing, Section } from "./ui";

export function MyCalls() {
  useTitle("My calls");
  const [, params] = useRoute("/my-calls/:round?");
  const [, go] = useLocation();
  let id = "";
  try {
    id = params?.round === undefined ? "" : decodeURIComponent(params.round);
  } catch {
    id = "%"; // undecodable: malformed, so nothing is requested
  }
  const shaped = roundIdShaped(id);
  const { session, reload } = useSession();
  const [reply, setReply] = useState<Reply<Mine> | null>(null);
  const [typed, setTyped] = useState("");
  const [deleted, setDeleted] = useState(false);

  useEffect(() => {
    if (!shaped || session.kind !== "in") {
      setReply(null);
      return;
    }
    let live = true;
    void mine(id).then((r) => {
      if (live) setReply(r);
    });
    return () => {
      live = false;
    };
  }, [id, shaped, session.kind]);

  return (
    <Section>
      <Heading kicker="Forecasting game">My calls</Heading>
      <div className="max-w-3xl space-y-6">
        {gameBase() === null ? (
          <GameOff />
        ) : (
          <>
            {deleted && session.kind !== "in" && (
              <p role="status" className="text-[var(--color-text)]">
                Your account was deleted and you are signed out. Your past calls stay in the public
                record under a random key, and nothing in the store links them to your
                X account.
              </p>
            )}
            <SessionBar session={session} onChange={reload} />
            {id === "" ? (
              <form
                className="flex flex-wrap items-center gap-3"
                onSubmit={(e) => {
                  e.preventDefault();
                  if (roundIdShaped(typed.trim())) go(`/my-calls/${encodeURIComponent(typed.trim())}`);
                }}
              >
                <label className="text-sm text-[var(--color-dim)]" htmlFor="my-round">
                  Round id
                </label>
                <input
                  id="my-round"
                  value={typed}
                  onChange={(e) => setTyped(e.target.value)}
                  className="min-w-0 flex-1 rounded-sm border border-[var(--color-line)] bg-[var(--color-surface)] px-3 py-2 text-[var(--color-text)]"
                />
                <button
                  type="submit"
                  className="display bg-[var(--color-signal)] px-5 py-2 text-[var(--color-ink)]"
                >
                  Show my calls
                </button>
              </form>
            ) : !shaped ? (
              <Nothing what="That is not a round id." why="Nothing was requested." />
            ) : session.kind === "in" ? (
              <Calls id={id} reply={reply} />
            ) : session.kind === "out" ? (
              <Nothing
                what="Sign in to see your calls."
                why="Your calls are shown only to you, and only while you are signed in."
              />
            ) : null}
            {session.kind === "in" && (
              <DeleteAccount
                csrf={session.csrf}
                onDeleted={() => {
                  setDeleted(true);
                  reload();
                }}
              />
            )}
          </>
        )}
      </div>
      <GameNotice />
    </Section>
  );
}

function Calls({ id, reply }: { id: string; reply: Reply<Mine> | null }) {
  if (reply === null) {
    return <p role="status" className="text-[var(--color-dim)]">Reading your calls…</p>;
  }
  if (reply.kind === "off" || reply.kind === "unreachable") {
    return <GameProblem what="Your calls could not be read." />;
  }
  if (reply.kind === "refused") {
    return <GameProblem what="Your calls could not be read." status={reply.status} error={reply.error} />;
  }
  const doc = reply.body;
  return (
    <div className="space-y-4">
      <p className="text-[var(--color-dim)]">
        Round <Here href={`/play/${encodeURIComponent(id)}`}>{doc.round}</Here>. Your board id is{" "}
        <code className="font-mono text-[var(--color-text)]">{doc.board_id}</code>; it is how your line
        appears on <Here href="/board">the board</Here>, with no handle beside it.
      </p>
      {doc.forecasts.length === 0 ? (
        <Nothing
          what="You have not called any coin in this round."
          why="Calls you file appear here at once."
        />
      ) : (
        <ul className="space-y-3">
          {doc.forecasts.map((c) => (
            <li key={`${c.chain}/${c.token}`}>
              <Card>
                <p className="text-xs tracking-widest text-[var(--color-faint)] uppercase">{c.chain}</p>
                <p className="font-mono text-sm break-all text-[var(--color-text)]">{c.token}</p>
                <p className="mt-2 text-sm text-[var(--color-dim)]">
                  You called it {c.side === "rug" ? "a rug" : "real"}, filed {isoOf(c.submitted_at) ?? "at an unknown time"}.
                  The window closes {isoOf(c.window_close) ?? "at an unknown time"}.
                </p>
              </Card>
            </li>
          ))}
        </ul>
      )}
      <ReadAge readAt={doc.read_at} newestAt={null} n={doc.forecasts.length} />
    </div>
  );
}
