// SPDX-License-Identifier: Apache-2.0
//! Money that is owed and money that moved, on one page.
//!
//! This was two pages, `/pool` and `/history`, and a reader asking "did it
//! actually pay out" had to open both to get one answer. Design 0025 §4 merges
//! them: the current pool at the top, every closed week below it. The two
//! components are unchanged; this page only stacks them, so each keeps its own
//! loading and fallback behaviour and its own tests.

import { History } from "./History";
import { Pool } from "./Pool";
import { useTitle } from "./title";

export function Payouts() {
  // Called after the two pages' own `useTitle`s have run -- a parent's effect
  // runs after its children's -- so the tab says what this page is.
  useTitle("Payouts");
  return (
    <>
      <Pool />
      <History />
    </>
  );
}
