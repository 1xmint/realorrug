// SPDX-License-Identifier: Apache-2.0
//! The retired payout process.
//!
//! The weekly prize is off (ADR 0038) and the token's fees pay for operations,
//! spent by the operator by hand (ADR 0037), so there is no pool for this to
//! pay out of. It refuses before reading any configuration or key: the Turnkey
//! signer it drove is retired, not repurposed as a treasury signer. The
//! library stays as the record of how past weeks were paid.

use std::process::ExitCode;

fn main() -> ExitCode {
    eprintln!(
        "realorrug-payout: retired -- the weekly prize is off (ADR 0038); fees are spent by hand (ADR 0037)"
    );
    ExitCode::FAILURE
}
