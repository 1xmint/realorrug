# Accepted replay cases

A case lands in this directory only after the owner has read `realorrug
replay`'s `review.md` for it and written `yes` on the accept line -- never
before, and never from a script.

Each accepted case is two files, sharing one stem:

- `<stem>.sheet.json` -- the capture `realorrug capture` wrote (or a
  hand-built one for a case that needs a specific fact combination), the same
  format `realorrug replay` reads.
- `<stem>.accepted.txt` -- the reply text the owner accepted for that
  capture, verbatim, with no trailing commentary.

`accepted_replies_still_pass.rs`, in the parent `tests/` directory, reads
every pair here, recomputes the level, the report and the reply from the
capture, and re-runs the three checks (`fidelity`, `forbidden`, `unknown`)
against the accepted reply text and against the freshly rendered report text.
An empty directory is not a gap in coverage -- it is the state before the
first case is ever accepted, and the test is written to pass over zero cases
for exactly that reason.

A case that starts failing here later means one of two things happened since
acceptance: the rules moved (`realorrug_roast::RULES_VERSION` bumped) in a
way that would refuse a reply the owner already signed off on, or a check
grew a new rule that this reply happens to trip. Either way the failure is
the point -- this directory is the regression suite for "would we still say
this," not a snapshot to keep green by editing the accepted text.
