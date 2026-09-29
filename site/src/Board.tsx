// SPDX-License-Identifier: Apache-2.0
//! The board: hit and miss counts with the number of calls behind them
//! (design 0032 Q1, §13).
//!
//! # What is deliberately not here
//!
//! Q1 (Josh, 2026-09-27): while the odds-by-level replay is unmeasured, the
//! board shows plain hit and miss counts with a sample size, and nothing
//! computed from them. So there is **no score, no rank, no winner and no
//! total**: the rows stay in the order the server sent them (by an opaque id,
//! which is not a ranking), nothing is sorted by hits, and no column adds
//! players together. A sort by hits is the first step to a winner.
//!
//! A line whose `n` is 0 is not drawn (the server already omits them), and an
//! empty board is [`Nothing`], never an empty table: an empty table says
//! players played and none hit.

import { useEffect, useState } from "react";

import { type Board as BoardDoc, type Reply, board, gameBase } from "./game";
import { GameNotice, GameOff, GameProblem, ReadAge } from "./GameUi";
import { useTitle } from "./title";
import { Heading, Nothing, Section } from "./ui";

export function Board() {
  useTitle("Board");
  const [reply, setReply] = useState<Reply<BoardDoc> | null>(null);

  useEffect(() => {
    if (gameBase() === null) return;
    let live = true;
    void board().then((r) => {
      if (live) setReply(r);
    });
    return () => {
      live = false;
    };
  }, []);

  return (
    <Section>
      <Heading kicker="Forecasting game">The board</Heading>
      <div className="max-w-3xl space-y-6">
        {gameBase() === null ? (
          <GameOff />
        ) : reply === null ? (
          <p role="status" className="text-[var(--color-dim)]">Reading the board…</p>
        ) : reply.kind === "unreachable" || reply.kind === "off" ? (
          <GameProblem what="The board could not be read." />
        ) : reply.kind === "refused" ? (
          <GameProblem what="The board could not be read." status={reply.status} error={reply.error} />
        ) : (
          <Lines doc={reply.body} />
        )}
      </div>
      <GameNotice />
    </Section>
  );
}

function Lines({ doc }: { doc: BoardDoc }) {
  const lines = doc.board.filter((l) => l.n > 0);
  return (
    <>
      <p className="text-[var(--color-dim)]">
        A hit is a call that matched how the coin was later settled; a miss is
        one that did not. A call that could not be settled is left out of the
        count. The counts are not ranked, not weighted and not added together,
        and a player appears only as an opaque id, never beside a handle.
      </p>
      {lines.length === 0 ? (
        <Nothing
          what="No settled calls yet."
          why="Nobody has a call that has been settled, so there is nothing to count. This is not a row of zeroes."
        />
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full text-left text-sm">
            <caption className="sr-only">Hit and miss counts, with the number of settled calls behind each</caption>
            <thead className="text-[var(--color-faint)]">
              <tr>
                <th scope="col" className="py-2 pr-4 font-normal">Player id</th>
                <th scope="col" className="py-2 pr-4 font-normal">Hits</th>
                <th scope="col" className="py-2 pr-4 font-normal">Misses</th>
                <th scope="col" className="py-2 font-normal">Settled calls (n)</th>
              </tr>
            </thead>
            <tbody className="text-[var(--color-text)]">
              {lines.map((l) => (
                <tr key={l.board_id} className="border-t border-[var(--color-line)]">
                  <th scope="row" className="py-2 pr-4 font-mono font-normal">{l.board_id}</th>
                  <td className="py-2 pr-4">{l.hits}</td>
                  <td className="py-2 pr-4">{l.misses}</td>
                  <td className="py-2">{l.n}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      <ReadAge
        readAt={doc.read_at}
        newestAt={doc.newest_at}
        newestLabel="Newest settled call"
        whenEmpty="No settled call yet."
      />
    </>
  );
}
