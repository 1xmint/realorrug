// SPDX-License-Identifier: Apache-2.0
//! What the forecasting game keeps about a person who signs in, exactly as the
//! server returns it (`GET /v1/privacy`).
//!
//! Rendered as returned, not paraphrased: the server is the one that stores
//! these fields, so its list is the list, and a page that reworded it could
//! drift from what is kept. The text is drawn as text only. If the server
//! cannot be reached the page says so and shows no notice of its own invention
//! about what is kept; the static Privacy page says only that signing in to the
//! game is different and points here.

import { useEffect, useState } from "react";

import { type PrivacyNotice, type Reply, gameBase, privacy } from "./game";
import { GameOff, GameProblem } from "./GameUi";
import { useTitle } from "./title";
import { Block, Heading, Here, Section } from "./ui";

export function GamePrivacy() {
  useTitle("Game privacy");
  const [reply, setReply] = useState<Reply<PrivacyNotice> | null>(null);

  useEffect(() => {
    if (gameBase() === null) return;
    let live = true;
    void privacy().then((r) => {
      if (live) setReply(r);
    });
    return () => {
      live = false;
    };
  }, []);

  return (
    <Section>
      <Heading kicker="Forecasting game">What playing keeps about you</Heading>
      <div className="max-w-2xl">
        {gameBase() === null ? (
          <GameOff />
        ) : reply === null ? (
          <p role="status" className="text-[var(--color-dim)]">Reading the notice…</p>
        ) : reply.kind !== "ok" ? (
          <GameProblem
            what="The privacy notice could not be read."
            {...(reply.kind === "refused" ? { status: reply.status, error: reply.error } : {})}
          />
        ) : (
          <Notice doc={reply.body} />
        )}
        <p className="mt-10 text-sm text-[var(--color-faint)]">
          The rest of this site is covered by <Here href="/privacy">its own privacy page</Here>.
        </p>
      </div>
    </Section>
  );
}

function Notice({ doc }: { doc: PrivacyNotice }) {
  return (
    <>
      {doc.kept && (
        <Block title="What is kept">
          <ul className="space-y-3">
            {doc.kept.map((k) => (
              <li key={k.field}>
                <strong className="text-[var(--color-text)]">{k.field}</strong>: {k.why}. Ends: {k.ends}.
              </li>
            ))}
          </ul>
        </Block>
      )}
      {doc.not_kept && (
        <Block title="What is not kept">
          <ul className="list-disc space-y-2 pl-5">
            {doc.not_kept.map((t) => (
              <li key={t}>{t}</li>
            ))}
          </ul>
        </Block>
      )}
      {doc.sessions && (
        <Block title="Sessions">
          <p>{doc.sessions}</p>
        </Block>
      )}
      {doc.deletion && (
        <Block title="Deleting your account">
          <p>{doc.deletion}</p>
        </Block>
      )}
      {doc.public && (
        <Block title="What is public">
          <p>{doc.public}</p>
        </Block>
      )}
    </>
  );
}
