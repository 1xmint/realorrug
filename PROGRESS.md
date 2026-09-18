# PROGRESS: the-bio-shows-the-whole-week

Done: Unit 1 complete and committed (5197d67). bio.rs rewritten so State
holds pool + leaders + last_winner together; status() drops leaders then
last winner in order until it fits; daemon.rs updated for the new
choose()/bio_to_write() signature (leaders param, empty Vec today).

Next: Unit 2 -- add `realorrug bio --preview` subcommand in
crates/realorrug-cli (lead from REALORRUG_BIO_LEAD or --lead; sample
state from --pool/--leader (repeatable)/--last-winner; print bio text +
length; tests for arg parsing and output == Bio::render). Then Unit 3
(short doc paragraph) and PROGRESS update, then push + open PR (do not
merge).

Watch out for: NEVER run cargo/tests locally per packet -- verify only
via CI after push. crates/realorrug-cli not yet examined; check its
existing arg-parsing framework (clap?) before adding the subcommand.
