// SPDX-License-Identifier: Apache-2.0
//! The forecasting game: a round, its coins, and the player's own calls
//! (design 0032 §13).
//!
//! # The page never reveals what the server hides
//!
//! Before a round closes the server answers with no count, no list and no
//! aggregate, so that nothing derived from a hidden call is observable (§11).
//! This page keeps that promise on its side too, in three ways:
//!
//! - it **does not ask** for the round's forecasts or outcomes until the round
//!   says `closed: true`, so an open round costs no request that could leak;
//! - it **does not draw** a forecast list unless that response says
//!   `closed: true` as well, so a server that answered early is not repeated;
//! - it **does not imply** them: no "0 calls so far", no "be the first", no
//!   hint that anyone else has or has not called. A count before close would be
//!   a fact about other players' calls, even a zero.
//!
//! After close, a coin shows how many calls were filed on it and never how they
//! split: what share of players called a coin one way is the crowd signal the
//! owner dropped (design 0032 Q2), and a per-side count is the same thing.

import { useEffect, useState } from "react";
import { Link, useLocation, useRoute } from "wouter";

import {
  type Coin,
  type Mine,
  type RoundForecasts,
  type RoundInfo,
  type RoundOutcomes,
  type Side,
  type Reply,
  fileCall,
  gameBase,
  isoOf,
  mine as fetchMine,
  roundIdShaped,
  rounds,
} from "./game";
import {
  GameNotice,
  GameOff,
  GameProblem,
  ReadAge,
  type Session,
  SessionBar,
  useSession,
} from "./GameUi";
import { useTitle } from "./title";
import { Card, Heading, Here, Nothing, Section } from "./ui";

/** `/play`: there is no server route that lists rounds, so a round is opened by its id. */
export function Play() {
  useTitle("Play");
  const [, go] = useLocation();
  const [id, setId] = useState("");
  const off = gameBase() === null;
  const trimmed = id.trim();
  const shaped = roundIdShaped(trimmed);

  return (
    <Section>
      <Heading kicker="Forecasting game">Call a round</Heading>
      <div className="max-w-2xl space-y-6">
        {off ? (
          <GameOff />
        ) : (
          <>
            <p className="text-[var(--color-dim)]">
              Each round lists some coins and a closing time. You call each coin
              real or rug before the window closes. The server&apos;s clock
              decides when it closed, and the first call on a coin stands.
            </p>
            <form
              className="flex flex-wrap items-center gap-3"
              onSubmit={(e) => {
                e.preventDefault();
                if (shaped) go(`/play/${encodeURIComponent(trimmed)}`);
              }}
            >
              <label className="text-sm text-[var(--color-dim)]" htmlFor="round-id">
                Round id
              </label>
              <input
                id="round-id"
                value={id}
                onChange={(e) => setId(e.target.value)}
                placeholder="the id the round was posted with"
                className="min-w-0 flex-1 rounded-sm border border-[var(--color-line)] bg-[var(--color-surface)] px-3 py-2 text-[var(--color-text)]"
              />
              <button
                type="submit"
                disabled={!shaped}
                className="display bg-[var(--color-signal)] px-5 py-2 text-[var(--color-ink)] disabled:opacity-40"
              >
                Open the round
              </button>
            </form>
            {trimmed !== "" && !shaped && (
              <p className="text-sm text-[var(--color-dim)]">
                A round id is letters, digits, dots, dashes and underscores.
              </p>
            )}
            <p className="text-sm text-[var(--color-faint)]">
              See <Here href="/board">the board</Here> for hit and miss counts.
            </p>
          </>
        )}
      </div>
      <GameNotice />
    </Section>
  );
}

/** The state of the public reads for one round. */
type Public =
  | { readonly kind: "loading" }
  | { readonly kind: "info"; readonly reply: Reply<RoundInfo> }
  | {
      readonly kind: "closed";
      readonly info: RoundInfo;
      readonly forecasts: Reply<RoundForecasts>;
      readonly outcomes: Reply<RoundOutcomes>;
    };

/** `/play/:round`. */
export function Round() {
  const [, params] = useRoute("/play/:round");
  const id = decodeURIComponent(params?.round ?? "");
  useTitle("Round");
  const shaped = roundIdShaped(id);
  const [seen, setSeen] = useState<Public>({ kind: "loading" });
  const { session, reload } = useSession();
  const [own, setOwn] = useState<Reply<Mine> | null>(null);
  const [ownTick, setOwnTick] = useState(0);

  useEffect(() => {
    if (!shaped || gameBase() === null) return;
    let live = true;
    setSeen({ kind: "loading" });
    void (async () => {
      const info = await rounds.info(id);
      if (!live) return;
      // Only a round that says it has closed is asked for its calls. An open
      // one stops here: see the module comment.
      if (info.kind === "ok" && info.body.closed === true) {
        const [forecasts, outcomes] = await Promise.all([
          rounds.forecasts(id),
          rounds.outcomes(id),
        ]);
        if (live) setSeen({ kind: "closed", info: info.body, forecasts, outcomes });
      } else {
        setSeen({ kind: "info", reply: info });
      }
    })();
    return () => {
      live = false;
    };
  }, [id, shaped]);

  // The player's own calls, whenever they are signed in. The author may read
  // their own hidden call before close (§9); nobody else's is ever asked for.
  useEffect(() => {
    if (!shaped || session.kind !== "in") {
      setOwn(null);
      return;
    }
    let live = true;
    void fetchMine(id).then((reply) => {
      if (live) setOwn(reply);
    });
    return () => {
      live = false;
    };
  }, [id, shaped, session.kind, ownTick]);

  return (
    <Section>
      <Heading kicker="Forecasting game">Round {id}</Heading>
      <div className="max-w-3xl space-y-6">
        {gameBase() === null ? (
          <GameOff />
        ) : !shaped ? (
          <Nothing
            what="That is not a round id."
            why="A round id is letters, digits, dots, dashes and underscores. Nothing was requested."
          />
        ) : (
          <>
            <SessionBar session={session} onChange={reload} />
            <RoundBody
              id={id}
              seen={seen}
              session={session}
              own={own}
              onFiled={() => setOwnTick((t) => t + 1)}
              onSignedOut={reload}
            />
            <p className="text-sm text-[var(--color-faint)]">
              <Link href={`/my-calls/${encodeURIComponent(id)}`} className="underline">
                Your calls in this round
              </Link>{" "}
              · <Link href="/board" className="underline">The board</Link>
            </p>
          </>
        )}
      </div>
      <GameNotice />
    </Section>
  );
}

function RoundBody({
  id,
  seen,
  session,
  own,
  onFiled,
  onSignedOut,
}: {
  id: string;
  seen: Public;
  session: Session;
  own: Reply<Mine> | null;
  onFiled: () => void;
  onSignedOut: () => void;
}) {
  if (seen.kind === "loading") {
    return <p role="status" className="text-[var(--color-dim)]">Reading the round…</p>;
  }
  if (seen.kind === "info") {
    const r = seen.reply;
    if (r.kind === "off") return <GameOff />;
    if (r.kind === "unreachable") return <GameProblem what="The round could not be read." />;
    if (r.kind === "refused") {
      return (
        <GameProblem
          what={r.status === 404 ? "There is no such round." : "The round could not be read."}
          status={r.status}
          error={r.error}
        />
      );
    }
    return (
      <OpenOrClosed
        id={id}
        info={r.body}
        session={session}
        own={own}
        outcomes={null}
        forecasts={null}
        onFiled={onFiled}
        onSignedOut={onSignedOut}
      />
    );
  }
  const o = seen.outcomes.kind === "ok" && seen.outcomes.body.closed === true ? seen.outcomes.body : null;
  const f = seen.forecasts.kind === "ok" && seen.forecasts.body.closed === true ? seen.forecasts.body : null;
  return (
    <OpenOrClosed
      id={id}
      info={seen.info}
      session={session}
      own={own}
      outcomes={o}
      forecasts={f}
      onFiled={onFiled}
      onSignedOut={onSignedOut}
    />
  );
}

function OpenOrClosed({
  id,
  info,
  session,
  own,
  outcomes,
  forecasts,
  onFiled,
  onSignedOut,
}: {
  id: string;
  info: RoundInfo;
  session: Session;
  own: Reply<Mine> | null;
  outcomes: RoundOutcomes | null;
  forecasts: RoundForecasts | null;
  onFiled: () => void;
  onSignedOut: () => void;
}) {
  const closes = isoOf(info.window_close);
  const mineByCoin = new Map<string, Mine["forecasts"][number]>();
  if (own?.kind === "ok") {
    for (const c of own.body.forecasts) mineByCoin.set(`${c.chain}/${c.token}`, c);
  }
  return (
    <div className="space-y-4">
      <p className="text-[var(--color-dim)]" data-window>
        {info.closed
          ? `The window closed at ${closes ?? "an unknown time"}.`
          : `The window closes at ${closes ?? "an unknown time"}, by the server's clock.`}
      </p>
      {!info.closed && (
        <p className="text-sm text-[var(--color-faint)]">
          Before the window closes this page shows nothing about anyone
          else&apos;s call, by design.
        </p>
      )}
      {own?.kind === "refused" && own.status !== 401 && (
        <GameProblem what="Your own calls could not be read." status={own.status} error={own.error} />
      )}
      <ul className="space-y-4">
        {info.coins.map((coin) => {
          const key = `${coin.chain}/${coin.token}`;
          const settled = outcomes?.outcomes?.find(
            (o) => o.chain === coin.chain && o.token === coin.token,
          );
          const filed = forecasts?.forecasts
            ? forecasts.forecasts.filter(
                (x) => x.chain === coin.chain && x.token === coin.token,
              ).length
            : null;
          return (
            <li key={key}>
              <Card>
                <p className="text-xs tracking-widest text-[var(--color-faint)] uppercase">
                  {coin.chain}
                </p>
                <p className="font-mono text-sm break-all text-[var(--color-text)]">{coin.token}</p>
                <div className="mt-4 space-y-2 text-sm text-[var(--color-dim)]">
                  <YourCall
                    id={id}
                    coin={coin}
                    info={info}
                    session={session}
                    mine={mineByCoin.get(key) ?? null}
                    ownKnown={own?.kind === "ok"}
                    onFiled={onFiled}
                    onSignedOut={onSignedOut}
                  />
                  {info.closed && (
                    <>
                      <p>
                        {settled
                          ? `Settled ${isoOf(settled.settled_at) ?? ""}: ${settled.reading}`
                          : "Not settled yet. Nothing is said about how it turned out."}
                      </p>
                      {filed !== null && <p>Calls filed on this coin: n = {filed}.</p>}
                    </>
                  )}
                </div>
              </Card>
            </li>
          );
        })}
      </ul>
      {info.closed && forecasts !== null && (
        <ReadAge readAt={forecasts.read_at} newestAt={forecasts.newest_at} n={forecasts.forecasts?.length ?? 0} newestLabel="Newest call" whenEmpty="No call was filed." />
      )}
      {info.closed && outcomes !== null && (
        <ReadAge readAt={outcomes.read_at} newestAt={outcomes.newest_at} n={outcomes.outcomes?.length ?? 0} newestLabel="Newest settlement" whenEmpty="Nothing has been settled yet." />
      )}
      {!info.closed && <ReadAge readAt={info.read_at} newestAt={null} />}
    </div>
  );
}

function YourCall({
  id,
  coin,
  info,
  session,
  mine,
  ownKnown,
  onFiled,
  onSignedOut,
}: {
  id: string;
  coin: Coin;
  info: RoundInfo;
  session: Session;
  mine: Mine["forecasts"][number] | null;
  ownKnown: boolean;
  onFiled: () => void;
  onSignedOut: () => void;
}) {
  if (mine !== null) {
    return (
      <p className="text-[var(--color-text)]" data-your-call>
        Your call: {mine.side === "rug" ? "rug" : "real"}, filed {isoOf(mine.submitted_at) ?? ""}. The first call
        on a coin stands.
      </p>
    );
  }
  if (session.kind !== "in") {
    if (info.closed) return null;
    return session.kind === "closed" ? (
      <p>Sign-in is not open yet, so this coin cannot be called.</p>
    ) : (
      <p>Sign in with X to call this coin.</p>
    );
  }
  if (!ownKnown) return <p>Reading your calls…</p>;
  if (info.closed) return <p>You did not call this coin.</p>;
  return (
    <CallForm
      id={id}
      coin={coin}
      csrf={session.csrf}
      onFiled={onFiled}
      onSignedOut={onSignedOut}
    />
  );
}

/**
 * The form that files one call.
 *
 * It sends the session cookie (`credentials: "include"`, in `game.ts`) and the
 * CSRF token the server issued at `/auth/me` as `x-csrf-token`. The body is the
 * round, the coin and the side and nothing else: the server refuses unknown
 * fields, and it decides the close and the odds itself.
 */
function CallForm({
  id,
  coin,
  csrf,
  onFiled,
  onSignedOut,
}: {
  id: string;
  coin: Coin;
  csrf: string;
  onFiled: () => void;
  onSignedOut: () => void;
}) {
  const [busy, setBusy] = useState(false);
  const [said, setSaid] = useState<string | null>(null);

  const file = async (side: Side) => {
    setBusy(true);
    setSaid(null);
    const reply = await fileCall(csrf, id, coin, side);
    setBusy(false);
    if (reply.kind === "ok") {
      onFiled();
    } else if (reply.kind === "refused" && reply.status === 409) {
      setSaid("You already called this coin. The first call stands.");
      onFiled();
    } else if (reply.kind === "refused" && reply.status === 401) {
      setSaid("Your session ended. Sign in again to file a call.");
      onSignedOut();
    } else if (reply.kind === "refused") {
      setSaid(reply.error ?? `The server refused the call (${reply.status}).`);
    } else {
      setSaid("The call may not have been saved. Reload to see whether it was.");
    }
  };

  return (
    <div>
      <div className="flex flex-wrap gap-3">
        <button
          type="button"
          disabled={busy}
          onClick={() => void file("real")}
          aria-label={`Call ${coin.token} real`}
          className="display bg-[var(--color-raised)] px-5 py-2 text-[var(--color-text)] disabled:opacity-40"
        >
          Real
        </button>
        <button
          type="button"
          disabled={busy}
          onClick={() => void file("rug")}
          aria-label={`Call ${coin.token} a rug`}
          className="display bg-[var(--color-raised)] px-5 py-2 text-[var(--color-text)] disabled:opacity-40"
        >
          Rug
        </button>
      </div>
      {said !== null && (
        <p role="alert" className="mt-2">
          {said}
        </p>
      )}
    </div>
  );
}
