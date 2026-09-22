Branch: who-paid-the-solana-buyers-v2 (based on main at 209d20c). PR #161 (draft), open at
https://github.com/1xmint/realorrug/pull/161

Done: rpc.rs's `signatures_page` primitive built and `signatures_back_to_oldest`
refactored onto it unchanged; wallets.rs's `funding_search` bounded backward
search implemented with `MAX_FUNDING_TRANSACTIONS = 10` and
`MAX_FUNDING_SIGNATURE_PAGES = 3`; `check_solana_candidate` rewritten to use it;
`funding_complete` is honest (found funder or measured end-of-history = true;
capped or failed = false, gap names which); 11 wallets.rs funding tests all
pass (fresh/busy/skip-then-find/dust-then-material/measured-absence/both
caps/failed-read-doesn't-abort-others); dossier.rs and sheet.rs call-count and
gap-message tests pass unchanged (verified, not modified); docs/design/0027 and
docs/research/0056 updated with the new behavior; clippy and fmt clean.

Next: PR #161 pushed and open as draft; CI just started (all checks pending as
of this write). Watch with `gh pr checks 161 --watch --interval 60` and fix any
failures (including cargo mutants surviving mutants CI reports) -- do not push
again until that run finishes.

Watch out for: never push while a watched CI run is in flight; if mutants
survive on the slot `>` comparison or either cap's `>=` comparison, add a
boundary test rather than loosen the check.
