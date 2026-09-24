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
        let mut from = 0;
        while let Some(at) = out[from..].find(marker) {
            let start = from + at + marker.len();
            let end = out[start..]
                .find(|c: char| c == '&' || c == '"' || c == '\'' || c.is_whitespace())
                .map_or(out.len(), |n| start + n);
            out.replace_range(start..end, "REDACTED");
            from = start + "REDACTED".len();
        }
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
}
