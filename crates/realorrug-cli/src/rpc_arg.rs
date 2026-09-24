// SPDX-License-Identifier: Apache-2.0
//! The Solana RPC endpoint for a command that must not guess one.
//!
//! [`RpcClient::from_vars`](realorrug_onchain::RpcClient::from_vars) falls
//! back to the public endpoint when nothing is configured, which suits the
//! read-anything commands (`dossier`, `roast`). A launch check or a treasury
//! reading is different: the public endpoint is rate-limited, so a missing
//! setting would surface as a refusal that reads like a finding about the
//! launch rather than about the configuration. Those commands require the
//! endpoint instead (AGENTS §3 rule 7), and never print it: a Helius endpoint
//! carries its key in the query string.

/// What stands in for the endpoint in anything printed.
pub(crate) const REDACTED: &str = "<rpc endpoint>";

/// `--rpc`, else `REALORRUG_RPC` (or the legacy `RADAR_RPC`), else an error.
///
/// # Errors
///
/// `"--rpc or REALORRUG_RPC is required"` when neither is set.
pub(crate) fn required_endpoint(
    args: &[String],
    get: &impl Fn(&str) -> Option<String>,
) -> Result<String, String> {
    crate::flag(args, "--rpc")
        .or_else(|| realorrug_types::env::env_or_legacy("REALORRUG_RPC", "RADAR_RPC", get))
        .filter(|e| !e.is_empty())
        .ok_or_else(|| "--rpc or REALORRUG_RPC is required".to_owned())
}

/// `text` with the endpoint, and any `api-key=`/`api_key=` value, replaced.
///
/// Both, because an error can quote the endpoint in a form other than the
/// one configured (ureq's bad-URI error quotes what it parsed): the whole
/// string catches the verbatim copy, the key pattern catches the rest.
pub(crate) fn redact(text: &str, endpoint: &str) -> String {
    let mut out = if endpoint.is_empty() {
        text.to_owned()
    } else {
        text.replace(endpoint, REDACTED)
    };
    for marker in ["api-key=", "api_key=", "API-KEY=", "API_KEY="] {
        // Rebuilt rather than edited in place: each pass consumes the text it
        // has looked at, so a marker is never found twice and the loop always
        // ends.
        let mut kept = String::with_capacity(out.len());
        let mut rest = out.as_str();
        while let Some(at) = rest.find(marker) {
            let (head, tail) = rest.split_at(at + marker.len());
            kept.push_str(head);
            kept.push_str("REDACTED");
            let end = tail
                .find(|c: char| c == '&' || c == '"' || c == '\'' || c.is_whitespace())
                .unwrap_or(tail.len());
            rest = &tail[end..];
        }
        kept.push_str(rest);
        out = kept;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn nothing_configured_is_refused_not_defaulted() {
        assert_eq!(
            required_endpoint(&args(&["x"]), &|_| None),
            Err("--rpc or REALORRUG_RPC is required".to_owned())
        );
        assert_eq!(
            required_endpoint(&args(&["x"]), &|_| Some(String::new())),
            Err("--rpc or REALORRUG_RPC is required".to_owned())
        );
    }

    #[test]
    fn the_flag_wins_over_the_environment() {
        let env = |k: &str| (k == "REALORRUG_RPC").then(|| "from-env".to_owned());
        assert_eq!(
            required_endpoint(&args(&["x", "--rpc", "from-flag"]), &env),
            Ok("from-flag".to_owned())
        );
        assert_eq!(
            required_endpoint(&args(&["x"]), &env),
            Ok("from-env".to_owned())
        );
    }

    #[test]
    fn neither_the_endpoint_nor_its_key_survives() {
        let endpoint = "rpc.example/?api-key=SENTINEL-7731";
        let text = format!("bad uri {endpoint}; again https://x/?api_key=SENTINEL-7731&a=1");
        let out = redact(&text, endpoint);
        assert!(!out.contains("SENTINEL-7731"), "{out}");
        assert!(out.contains(REDACTED), "{out}");
        assert!(out.ends_with("api_key=REDACTED&a=1"), "{out}");
    }

    /// A key ends at whatever an error message puts after a URL: a quote,
    /// an apostrophe or a space, as well as the next query parameter.
    #[test]
    fn a_key_ends_at_a_quote_an_apostrophe_or_a_space() {
        let text = "a \"x/?api-key=K1\" b 'y/?api_key=K2' c z/?API-KEY=K3 d";
        assert_eq!(
            redact(text, ""),
            "a \"x/?api-key=REDACTED\" b 'y/?api_key=REDACTED' c z/?API-KEY=REDACTED d"
        );
    }
}
