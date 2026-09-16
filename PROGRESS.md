# Progress — packet 0039 (task 9-15-0036, branch feat/0038-memory-in-front)

- [ ] dossier::build takes `memory: Option<&Memory>`, caches only the launch
      record (what="launch", subject=mint, block=launch block), skips
      signature paging on a hit, records on a miss, surfaces record's
      Conflict refusal as a "launch block" miss.
- [ ] realorrug-cli's dossier.rs updated to pass `None` for memory (no
      existing state-path config to open one from).
- [ ] lib.rs / SolanaReader wiring updated to match build's new signature.
- [ ] design 0021 updated: §1 boundary (only launch cached, why the other
      two are deferred + the condition that unblocks them: a sheet read
      point per fact, not one for the whole sheet) and §3 boundary (memory
      is never the source of a fact the bot could not read today).
- [ ] Tests 1-5 from the packet, each verified by deleting the guard it
      covers and watching it fail.
- [ ] cargo test -p realorrug-onchain, build checks for -p realorrug-cli,
      -p realorrug-roast, -p realorrug-analyst.
- [ ] clippy --all-targets -D warnings on edited crates.
- [ ] cargo test -p repo-conformance.
- [ ] cargo fmt --check as the very last command before push.

Done: nothing committed yet, this checkpoint file only.
Next: implement memory.rs plumbing into dossier::build.
Watch out for: rustfmt fn_call_width is 60, not 100 — run fmt --check after
the FINAL edit, not before.
