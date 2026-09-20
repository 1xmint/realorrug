// SPDX-License-Identifier: Apache-2.0
//! `GET /v1/facts/{token}` — the paid, facts-only endpoint (ADR 0036).
//!
//! Sells exactly the same measured facts the free bot already publishes per
//! token: [`FactSheet`]'s facts (each price with the block/time it was read
//! at, ADR 0033) and [`realorrug_roast::sheet::factors`]'s signals with their
//! evidence and grade. It never returns [`realorrug_roast::Assessment`]'s
//! `risk_index`, `score_bps`, `level` or `score_level`, and never a factor's
//! `delta_bps` — that number is the per-factor weight the level is built
//! from, held back with the level itself until calibration (research 0052,
//! ADR 0036 decision 1). There is no model call anywhere on this path:
//! [`FactSheet::build`] is the same model-free call `check.rs` and `roast
//! --sheet` already use.
//!
//! # Deny by default: no pay-to, no route
//!
//! [`FactsState::from_vars`] returns `None` when `REALORRUG_X402_PAY_TO` is
//! unset. [`crate::app`] merges [`router`] only when it is `Some`, so an
//! unconfigured box 404s the ordinary axum way for every path under
//! `/v1/facts` — the same pattern `realorrug-agent`'s doc comment already
//! names for the x402 surface.
//!
//! # Settle after the facts, never before
//!
//! The handler verifies the payment first (checks the claim against the
//! price, asset and network; moves no money), then reads the chain and
//! builds the sheet, and only settles — the one step that moves money — once
//! the response body exists. A failed read (bad address, no endpoint
//! configured, the chain call errors) returns its error status and never
//! calls [`Facilitator::settle`], so a buyer is never charged for an answer
//! they did not get (ADR 0036 decision 5).
//!
//! # No `x402-rs`
//!
//! ADR 0036 decision 6 explains why this hand-rolls the x402 JSON shapes
//! instead of taking the crate research 0054 named: the actual verification
//! and settlement happen at the facilitator, over HTTP, so there is little
//! wire format left to get wrong here.

use std::fmt::Write as _;
use std::sync::Arc;
use std::time::SystemTime;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use realorrug_onchain::{Memory, PaidRequest, RpcClient, dispatch};
use realorrug_roast::sheet::factors;
use realorrug_roast::{BaseRates, FactSheet};
use realorrug_types::ChainAddress;
use realorrug_types::env::env_or_legacy;
use serde_json::{Value, json};
use sha3::{Digest, Keccak256};

/// The fixed price of one call: $0.05, in USDC's atomic unit (6 decimals).
///
/// A constant, not an environment variable — ADR 0036 decision 2: the price
/// is this PR's decision to record, not an operator's knob to drift.
pub const PRICE_ATOMIC_USDC: &str = "50000";

/// The network payment settles on. Distinct from the chain the *token* being
/// described lives on (Robinhood Chain) — ADR 0036's context section.
pub const NETWORK: &str = "base";

/// The x402 scheme this endpoint accepts.
pub const SCHEME: &str = "exact";

/// What a facilitator answered about a payment claim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifyOutcome {
    /// Whether the claim is good against the requirements it was checked
    /// against.
    pub valid: bool,
    /// The payer's address, once the facilitator has told us one. `None`
    /// before or on a failed verify — a payer is never guessed at.
    pub payer: Option<String>,
    /// Why verification failed, for the 402 body's `error` field.
    pub reason: Option<String>,
}

/// What a facilitator answered about a settlement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettleReceipt {
    /// The settlement transaction hash.
    pub tx_hash: String,
    /// The payer's address, as the facilitator's settlement reported it.
    pub payer: String,
    /// The settled amount, in the asset's smallest unit.
    pub amount: String,
}

/// The seam between this route and the payment network.
///
/// A trait, not a direct `ureq` call inline, so tests exercise the
/// verify-build-settle order (ADR 0036 decision 5) with a fake that makes no
/// real network call — the packet's own requirement.
pub trait Facilitator: Send + Sync {
    /// Checks a claim (the `X-PAYMENT` header's value) against the payment
    /// requirements this route published. Moves no money.
    fn verify(&self, payment_header: &str, requirements: &Value) -> VerifyOutcome;

    /// Settles a previously verified claim. The one step that moves money.
    ///
    /// # Errors
    ///
    /// A message describing why settlement failed (the facilitator refused,
    /// timed out, or otherwise did not confirm).
    fn settle(&self, payment_header: &str, requirements: &Value) -> Result<SettleReceipt, String>;
}

/// A [`Facilitator`] reached over HTTP.
///
/// **The exact field names below are unverified against a live facilitator**
/// (research 0054 flagged `x402-rs`, the crate that would have pinned them,
/// as itself unread this session) — named here as the honest gap ADR 0036
/// decision 6 records, not hidden. Nothing in this PR deploys this type;
/// wiring a real facilitator URL in is the owner's action.
pub struct HttpFacilitator {
    base_url: String,
}

impl HttpFacilitator {
    /// Builds a facilitator that calls `base_url`'s `/verify` and `/settle`.
    #[must_use]
    pub fn new(base_url: String) -> Self {
        Self { base_url }
    }
}

impl Facilitator for HttpFacilitator {
    fn verify(&self, payment_header: &str, requirements: &Value) -> VerifyOutcome {
        let body = json!({ "x402Version": 1, "paymentHeader": payment_header, "paymentRequirements": requirements });
        let response = ureq::post(format!("{}/verify", self.base_url))
            .send_json(&body)
            .and_then(|mut r| r.body_mut().read_json::<Value>());
        match response {
            Ok(doc) => verify_outcome(&doc),
            Err(e) => VerifyOutcome {
                valid: false,
                payer: None,
                reason: Some(format!("facilitator unreachable: {e}")),
            },
        }
    }

    fn settle(&self, payment_header: &str, requirements: &Value) -> Result<SettleReceipt, String> {
        let body = json!({ "x402Version": 1, "paymentHeader": payment_header, "paymentRequirements": requirements });
        let doc = ureq::post(format!("{}/settle", self.base_url))
            .send_json(&body)
            .and_then(|mut r| r.body_mut().read_json::<Value>())
            .map_err(|e| format!("facilitator unreachable: {e}"))?;
        settle_receipt(&doc)
    }
}

/// Reads a facilitator's `/verify` answer. Split from the HTTP call above so
/// it can be tested without a network: a facilitator's "no" arriving as a
/// "yes" is the one reading mistake here that would give facts away free.
fn verify_outcome(doc: &Value) -> VerifyOutcome {
    VerifyOutcome {
        // Absent is not valid (AGENTS rule 8): a body with no `isValid` at
        // all is refused, never read as approval.
        valid: doc.get("isValid").and_then(Value::as_bool).unwrap_or(false),
        payer: doc.get("payer").and_then(Value::as_str).map(str::to_owned),
        reason: doc
            .get("invalidReason")
            .and_then(Value::as_str)
            .map(str::to_owned),
    }
}

/// Reads a facilitator's `/settle` answer. `Err` whenever it did not say
/// `success: true` -- including when it said nothing at all, which is the
/// deny-by-default reading, not an optimistic one.
fn settle_receipt(doc: &Value) -> Result<SettleReceipt, String> {
    if !doc.get("success").and_then(Value::as_bool).unwrap_or(false) {
        return Err(doc
            .get("errorReason")
            .and_then(Value::as_str)
            .unwrap_or("settlement refused")
            .to_owned());
    }
    Ok(SettleReceipt {
        tx_hash: doc
            .get("transaction")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        payer: doc
            .get("payer")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        amount: PRICE_ATOMIC_USDC.to_owned(),
    })
}

/// State for the paid facts route.
pub struct FactsState {
    /// The receive-only address payment settles to. Never a signing key —
    /// its presence is only ever read, never used to construct one (ADR
    /// 0036 decision 3).
    pub pay_to: String,
    /// The asset contract this route requires payment in. `None` refuses
    /// every request with 503 rather than guess at Base's USDC contract
    /// address without an operator having confirmed it (rule 7).
    pub asset: Option<String>,
    /// Solana's endpoint, always set the same way `CheckState`'s is.
    pub solana_endpoint: String,
    /// Robinhood Chain's endpoint, or `None` when unconfigured.
    pub robinhood_endpoint: Option<String>,
    /// The base rates a sheet is built against.
    pub rates: Option<BaseRates>,
    /// Where the daemon's SQLite memory lives, so a settled request can be
    /// recorded there (ADR 0036 decision 9: no new database).
    pub memory_path: Option<std::path::PathBuf>,
    /// The facilitator this route calls to verify and settle. `None` refuses
    /// every request with 503 — a route with a pay-to address but no
    /// facilitator cannot honestly clear a payment.
    pub facilitator: Option<Arc<dyn Facilitator>>,
}

impl FactsState {
    /// Builds the state from the environment. Returns `None` when
    /// `REALORRUG_X402_PAY_TO` is unset — the caller must not mount the
    /// route in that case (ADR 0036 decision 4).
    #[must_use]
    pub fn from_vars(get: &impl Fn(&str) -> Option<String>) -> Option<Self> {
        let pay_to = env_or_legacy("REALORRUG_X402_PAY_TO", "RADAR_X402_PAY_TO", get)?;
        let asset = env_or_legacy("REALORRUG_X402_ASSET", "RADAR_X402_ASSET", get);
        let solana_endpoint = RpcClient::from_vars(get).endpoint().to_owned();
        let robinhood_endpoint =
            env_or_legacy("REALORRUG_ROBINHOOD_RPC", "RADAR_ROBINHOOD_RPC", get);
        let rates = BaseRates::load(
            &env_or_legacy("REALORRUG_BASE_RATES", "RADAR_BASE_RATES", get)
                .unwrap_or_else(|| realorrug_roast::baserates::DEFAULT_PATH.to_owned()),
        )
        .ok();
        let analyst_dir = env_or_legacy("REALORRUG_ANALYST_DIR", "RADAR_ANALYST_DIR", get)
            .unwrap_or_else(|| "data/analyst".to_owned());
        let memory_path = Some(std::path::PathBuf::from(format!(
            "{analyst_dir}/memory.sqlite3"
        )));
        let facilitator_url = env_or_legacy(
            "REALORRUG_X402_FACILITATOR_URL",
            "RADAR_X402_FACILITATOR_URL",
            get,
        );
        let facilitator: Option<Arc<dyn Facilitator>> =
            facilitator_url.map(|url| Arc::new(HttpFacilitator::new(url)) as Arc<dyn Facilitator>);
        Some(Self {
            pay_to,
            asset,
            solana_endpoint,
            robinhood_endpoint,
            rates,
            memory_path,
            facilitator,
        })
    }

    fn payment_requirements(&self, token: &str) -> Value {
        json!({
            "scheme": SCHEME,
            "network": NETWORK,
            "maxAmountRequired": PRICE_ATOMIC_USDC,
            "resource": format!("/v1/facts/{token}"),
            "description": "Measured, facts-only data for one Robinhood Chain token: no score, no level, no model output.",
            "mimeType": "application/json",
            "payTo": self.pay_to,
            "maxTimeoutSeconds": 60,
            "asset": self.asset,
        })
    }
}

/// The router for this route alone, merged into [`crate::app`] only when
/// [`FactsState::from_vars`] returned `Some`.
pub fn router(state: Arc<FactsState>) -> Router {
    Router::new()
        .route("/v1/facts/{token}", get(handle))
        .with_state(state)
}

async fn handle(
    State(state): State<Arc<FactsState>>,
    Path(token): Path<String>,
    headers: HeaderMap,
) -> Response {
    let requirements = state.payment_requirements(&token);

    let payment_header = headers
        .get("X-PAYMENT")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let Some(payment_header) = payment_header else {
        return payment_required(&requirements, "X-PAYMENT header is required");
    };

    let parsed: Result<ChainAddress, _> = token.parse();
    let Ok(address @ ChainAddress::Robinhood(_)) = parsed else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "this endpoint reads Robinhood Chain tokens only, as a 0x + 40 hex address" })),
        )
            .into_response();
    };

    let Some(facilitator) = state.facilitator.clone() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": "no payment facilitator is configured" })),
        )
            .into_response();
    };

    if let Err(refusal) = verify(&facilitator, &payment_header, &requirements).await {
        return *refusal;
    }

    // Verified but not yet settled. Build the sheet before touching money --
    // ADR 0036 decision 5.
    let dossier = match read_dossier(&state, &address.to_string()).await {
        Ok(dossier) => dossier,
        Err(refusal) => return *refusal,
    };

    let sheet = FactSheet::build(&dossier, state.rates.as_ref(), None, None, None);
    let body = facts_doc(&token, &sheet);
    let response_hash = hash_response(&body);

    // Settle only now that the facts exist -- a failed read above already
    // returned without reaching this line.
    let receipt = match settle(&facilitator, &payment_header, &requirements).await {
        Ok(receipt) => receipt,
        Err(refusal) => return *refusal,
    };

    record_paid_request(&state, &token, &sheet, &response_hash, &receipt);
    // The parsed address, not the `{token}` path segment, for the same
    // reason `check.rs` uses it: one token must have one spelling in the
    // record or its history splits in two.
    //
    // The level is computed here for the record alone and never for the body:
    // ADR 0036 decision 1 holds it back from the paid response until
    // calibration. Recording it anyway is what makes the calibration possible.
    crate::record::verdict(
        state.memory_path.as_deref(),
        "robinhood",
        &address.to_string(),
        &sheet,
        crate::record::level_name(realorrug_roast::verdict::Verdict::from(&sheet).level),
        "facts",
    );
    crate::record::token_text(
        state.memory_path.as_deref(),
        "robinhood",
        &address.to_string(),
        &dossier,
    );

    let mut response = (StatusCode::OK, Json(body)).into_response();
    response.headers_mut().insert(
        "X-PAYMENT-RESPONSE",
        format!(
            "{{\"success\":true,\"transaction\":\"{}\"}}",
            receipt.tx_hash
        )
        .parse()
        .expect("ascii header value"),
    );
    response
}

/// Checks the claim against the published requirements. `Err` carries the
/// 402 to return; moves no money either way.
async fn verify(
    facilitator: &Arc<dyn Facilitator>,
    payment_header: &str,
    requirements: &Value,
) -> Result<(), Box<Response>> {
    let owned = (
        facilitator.clone(),
        payment_header.to_owned(),
        requirements.clone(),
    );
    let outcome = tokio::task::spawn_blocking(move || owned.0.verify(&owned.1, &owned.2)).await;
    match outcome {
        Ok(outcome) if outcome.valid => Ok(()),
        Ok(outcome) => Err(Box::new(payment_required(
            requirements,
            outcome.reason.as_deref().unwrap_or("payment invalid"),
        ))),
        Err(_join_error) => Err(Box::new(payment_required(
            requirements,
            "payment verification did not finish",
        ))),
    }
}

/// Reads the chain for one token. `Err` carries the error response to
/// return, which is always returned *before* [`settle`] is reached, so a
/// failed read never charges the buyer.
async fn read_dossier(
    state: &Arc<FactsState>,
    token_text: &str,
) -> Result<realorrug_onchain::Dossier, Box<Response>> {
    let solana_endpoint = state.solana_endpoint.clone();
    let robinhood_endpoint = state.robinhood_endpoint.clone();
    let token_text = token_text.to_owned();
    let read = tokio::task::spawn_blocking(move || {
        let solana = RpcClient::new(solana_endpoint);
        let robinhood = robinhood_endpoint
            .as_deref()
            .map(realorrug_robinhood::Rpc::new);
        let clients = dispatch::Clients {
            solana: &solana,
            robinhood: robinhood.as_ref(),
            market: None,
        };
        dispatch::read(&token_text, &clients)
    })
    .await;

    match read {
        Ok(Ok(dossier)) => Ok(dossier),
        Ok(Err(dispatch::Error::NotAnAddress)) => Err(Box::new(
            (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "not a Robinhood Chain address" })),
            )
                .into_response(),
        )),
        Ok(Err(dispatch::Error::Unreadable(why))) => Err(Box::new(
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "error": why })),
            )
                .into_response(),
        )),
        Err(_join_error) => Err(Box::new(
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "error": "the read did not finish" })),
            )
                .into_response(),
        )),
    }
}

/// Settles the verified claim: the one step that moves money. `Err` carries
/// the response to return when it did not.
async fn settle(
    facilitator: &Arc<dyn Facilitator>,
    payment_header: &str,
    requirements: &Value,
) -> Result<SettleReceipt, Box<Response>> {
    let owned = (
        facilitator.clone(),
        payment_header.to_owned(),
        requirements.clone(),
    );
    let receipt = tokio::task::spawn_blocking(move || owned.0.settle(&owned.1, &owned.2)).await;
    match receipt {
        Ok(Ok(receipt)) => Ok(receipt),
        Ok(Err(reason)) => Err(Box::new(
            (
                StatusCode::PAYMENT_REQUIRED,
                Json(json!({ "error": format!("settlement failed: {reason}") })),
            )
                .into_response(),
        )),
        Err(_join_error) => Err(Box::new(
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "error": "settlement did not finish" })),
            )
                .into_response(),
        )),
    }
}

fn payment_required(requirements: &Value, reason: &str) -> Response {
    (
        StatusCode::PAYMENT_REQUIRED,
        Json(json!({
            "x402Version": 1,
            "accepts": [requirements],
            "error": reason,
        })),
    )
        .into_response()
}

/// The facts-only JSON body: [`FactSheet`]'s facts, read point, coverage
/// gaps and the signals' evidence -- never [`realorrug_roast::Assessment`]'s
/// score or level, and never a factor's `delta_bps` (ADR 0036 decision 1).
fn facts_doc(token: &str, sheet: &FactSheet) -> Value {
    let public_factors: Vec<Value> = factors(sheet)
        .into_iter()
        .map(|f| {
            json!({
                "signal": f.signal,
                "name": f.name,
                "grade": f.grade,
                "evidence": f.evidence,
            })
        })
        .collect();
    let public_facts: Vec<Value> = sheet
        .facts
        .iter()
        .map(|fact| {
            json!({
                "about": fact.about,
                "label": fact.label,
                "rendered": fact.rendered,
                "values": fact.values,
            })
        })
        .collect();
    json!({
        "token": token,
        "read_at": sheet.read_at,
        "facts": public_facts,
        "unknown": sheet.unknown,
        "skipped": sheet.skipped,
        "signals": public_factors,
    })
}

/// A `keccak256` hash of the exact response body served, hex-encoded --
/// so a later dispute can prove what was sent without storing the whole
/// payload a second time (`PaidRequest::response_hash`'s own doc comment).
fn hash_response(body: &Value) -> String {
    let bytes = serde_json::to_vec(body).unwrap_or_default();
    let digest = Keccak256::digest(&bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Records the settled request in the daemon's SQLite memory. A recording
/// failure (no memory path configured, or the write itself fails) never
/// un-serves an already-settled response -- the buyer paid and got their
/// answer; this is best-effort bookkeeping for the moat, not a gate on
/// serving (`docs/adr/0036`'s decision 10 covers what is recorded, not
/// what happens if the local write fails).
fn record_paid_request(
    state: &FactsState,
    token: &str,
    sheet: &FactSheet,
    response_hash: &str,
    receipt: &SettleReceipt,
) {
    let Some(path) = state.memory_path.as_ref() else {
        return;
    };
    let Ok(memory) = Memory::open(path) else {
        return;
    };
    let _ = memory.record_paid_request(&PaidRequest {
        chain: "robinhood".to_owned(),
        token: token.to_owned(),
        read_at_block: sheet.read_at.and_then(|r| match r {
            realorrug_types::ReadAt::Robinhood(block) => Some(block),
            realorrug_types::ReadAt::Solana(_) => None,
        }),
        response_hash: response_hash.to_owned(),
        payer: receipt.payer.clone(),
        amount: receipt.amount.clone(),
        tx_hash: receipt.tx_hash.clone(),
        served_at: SystemTime::now(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;
    use realorrug_onchain::Dossier;
    use realorrug_onchain::dossier::Powers;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn no_env(_: &str) -> Option<String> {
        None
    }

    /// A facilitator that makes no network call and counts what it was asked
    /// to do, so a test can assert the *order* ADR 0036 decision 5 fixes:
    /// settle is reached only after the facts exist.
    struct FakeFacilitator {
        valid: bool,
        verifies: AtomicUsize,
        settles: AtomicUsize,
    }

    impl FakeFacilitator {
        fn new(valid: bool) -> Arc<Self> {
            Arc::new(Self {
                valid,
                verifies: AtomicUsize::new(0),
                settles: AtomicUsize::new(0),
            })
        }
    }

    impl Facilitator for FakeFacilitator {
        fn verify(&self, _payment_header: &str, _requirements: &Value) -> VerifyOutcome {
            self.verifies.fetch_add(1, Ordering::SeqCst);
            VerifyOutcome {
                valid: self.valid,
                payer: self.valid.then(|| "0xpayer".to_owned()),
                reason: (!self.valid).then(|| "the claim does not pay enough".to_owned()),
            }
        }

        fn settle(
            &self,
            _payment_header: &str,
            _requirements: &Value,
        ) -> Result<SettleReceipt, String> {
            self.settles.fetch_add(1, Ordering::SeqCst);
            Ok(SettleReceipt {
                tx_hash: "0xsettlement".to_owned(),
                payer: "0xpayer".to_owned(),
                amount: PRICE_ATOMIC_USDC.to_owned(),
            })
        }
    }

    /// No Robinhood endpoint and no memory path: a chain read cannot succeed
    /// here, which is exactly what the "a failed read never settles" test
    /// needs, and no test ever touches the network or a database file.
    fn state_with(facilitator: Option<Arc<dyn Facilitator>>) -> Arc<FactsState> {
        Arc::new(FactsState {
            pay_to: "0x0000000000000000000000000000000000000001".to_owned(),
            asset: Some("0x0000000000000000000000000000000000000002".to_owned()),
            solana_endpoint: "http://127.0.0.1:1".to_owned(),
            robinhood_endpoint: None,
            rates: None,
            memory_path: None,
            facilitator,
        })
    }

    async fn call(
        state: Arc<FactsState>,
        token: &str,
        payment_header: Option<&str>,
    ) -> (StatusCode, Value) {
        let mut headers = HeaderMap::new();
        if let Some(value) = payment_header {
            headers.insert("X-PAYMENT", value.parse().expect("ascii header"));
        }
        let response = handle(State(state), Path(token.to_owned()), headers).await;
        let status = response.status();
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("a body")
            .to_bytes();
        let body: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, body)
    }

    fn a_token() -> &'static str {
        "0x00000000000000000000000000000000000000aa"
    }

    fn dossier_with_powers() -> Dossier {
        Dossier {
            mint: a_token().parse().expect("a Robinhood address"),
            read_at: None,
            launch: None,
            curve: None,
            creator_transactions: None,
            chain_launch: None,
            holders: None,
            funding: None,
            market: None,
            token_ownership: None,
            creator_cash_flow: None,
            powers: Some(Powers {
                creator_tax_bps: 900,
                pending_creator_fee_recipient: None,
                exemptions: Vec::new(),
            }),
            unavailable: Vec::new(),
            calls: 0,
            elapsed_ms: 0,
        }
    }

    /// Deny by default (ADR 0036 decision 4): `REALORRUG_X402_PAY_TO` alone
    /// decides whether the route exists at all. Unset means `None`, which
    /// means `app()` merges nothing and every `/v1/facts` path 404s. The
    /// second half pins that it is *that* variable and not another: with only
    /// the pay-to set, and no asset and no facilitator, the state still
    /// builds.
    #[test]
    fn no_pay_to_means_no_state_at_all() {
        assert!(FactsState::from_vars(&no_env).is_none());
        let only_pay_to =
            |key: &str| (key == "REALORRUG_X402_PAY_TO").then(|| "0xreceive".to_owned());
        let state = FactsState::from_vars(&only_pay_to).expect("a pay-to mounts the route");
        assert_eq!(state.pay_to, "0xreceive");
        assert!(state.asset.is_none(), "an unset asset is not guessed at");
        assert!(
            state.facilitator.is_none(),
            "an unset facilitator URL is not guessed at"
        );
    }

    /// An unpaid request is answered 402 with the x402 offer: the price, the
    /// asset, the network and where to pay. The price assertion is the
    /// boundary one -- `PRICE_ATOMIC_USDC` is USDC's atomic unit at six
    /// decimals, so a digit dropped or added here charges 0.5 cents or 50
    /// cents while every other test still passes.
    #[tokio::test]
    async fn an_unpaid_request_is_offered_the_price() {
        let facilitator = FakeFacilitator::new(true);
        let state = state_with(Some(Arc::clone(&facilitator) as Arc<dyn Facilitator>));
        let (status, body) = call(state, a_token(), None).await;

        assert_eq!(status, StatusCode::PAYMENT_REQUIRED);
        assert_eq!(body["x402Version"], 1);
        let offer = &body["accepts"][0];
        assert_eq!(offer["scheme"], SCHEME);
        assert_eq!(offer["network"], NETWORK);
        assert_eq!(offer["payTo"], state_with(None).pay_to);
        assert_eq!(offer["resource"], format!("/v1/facts/{}", a_token()));
        assert_eq!(
            offer["maxAmountRequired"], PRICE_ATOMIC_USDC,
            "the offer must quote $0.05, which is 50000 at USDC's six decimals"
        );
        assert_eq!(PRICE_ATOMIC_USDC, "50000");
        assert_eq!(
            facilitator.verifies.load(Ordering::SeqCst),
            0,
            "a request with no X-PAYMENT header never reaches the facilitator"
        );
        assert_eq!(facilitator.settles.load(Ordering::SeqCst), 0);
    }

    /// A claim the facilitator refuses is answered 402 with the reason, and
    /// nothing is read and nothing is settled.
    #[tokio::test]
    async fn a_refused_claim_never_reaches_settlement() {
        let facilitator = FakeFacilitator::new(false);
        let state = state_with(Some(Arc::clone(&facilitator) as Arc<dyn Facilitator>));
        let (status, body) = call(state, a_token(), Some("a-claim")).await;

        assert_eq!(status, StatusCode::PAYMENT_REQUIRED);
        assert_eq!(body["error"], "the claim does not pay enough");
        assert_eq!(facilitator.verifies.load(Ordering::SeqCst), 1);
        assert_eq!(
            facilitator.settles.load(Ordering::SeqCst),
            0,
            "an invalid payment is never settled"
        );
    }

    /// ADR 0036 decision 5, the rule that costs the buyer money if it breaks:
    /// a verified payment whose chain read then fails returns the read's error
    /// and settles nothing. Moving the settle call above the read would leave
    /// this test's settle count at 1 -- that is the bug being re-applied.
    #[tokio::test]
    async fn a_failed_read_is_never_charged_for() {
        let facilitator = FakeFacilitator::new(true);
        // No Robinhood endpoint configured, so the read fails before any
        // network call is attempted.
        let state = state_with(Some(Arc::clone(&facilitator) as Arc<dyn Facilitator>));
        let (status, _body) = call(state, a_token(), Some("a-claim")).await;

        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            facilitator.verifies.load(Ordering::SeqCst),
            1,
            "the payment was checked"
        );
        assert_eq!(
            facilitator.settles.load(Ordering::SeqCst),
            0,
            "a buyer who got no answer is never charged"
        );
    }

    /// What is sold is facts, never the score (ADR 0036 decision 1). The
    /// sheet here carries a live creator tax, which is enough for at least one
    /// signal to appear -- so the "no `delta_bps`" assertion is about a factor
    /// that actually exists, not about an empty list.
    #[test]
    fn the_sold_document_carries_signals_but_no_score() {
        let dossier = dossier_with_powers();
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert!(
            !factors(&sheet).is_empty(),
            "a live creator tax must produce at least one signal to test against"
        );

        let doc = facts_doc(a_token(), &sheet);
        let signals = doc["signals"].as_array().expect("a signals array");
        assert!(!signals.is_empty());
        for signal in signals {
            let keys: Vec<&str> = signal
                .as_object()
                .expect("an object")
                .keys()
                .map(String::as_str)
                .collect();
            assert_eq!(keys, ["evidence", "grade", "name", "signal"]);
        }

        let serialized = serde_json::to_string(&doc).expect("json");
        for withheld in ["delta_bps", "score_bps", "risk_index", "level"] {
            assert!(
                !serialized.contains(withheld),
                "{withheld} is held back until calibration, but the body carries it: {serialized}"
            );
        }
    }

    /// A settlement is a receipt only when the facilitator says so in as many
    /// words. Reading this backwards -- dropping the `!` -- would turn every
    /// refusal into a receipt: the endpoint would hand over the facts and
    /// record a sale for a payment that never cleared.
    #[test]
    fn only_an_explicit_success_is_a_receipt() {
        assert_eq!(
            settle_receipt(&json!({ "success": false, "errorReason": "insufficient funds" })),
            Err("insufficient funds".to_owned())
        );
        assert_eq!(
            settle_receipt(&json!({ "transaction": "0xabc" })),
            Err("settlement refused".to_owned()),
            "a body that never says success is refused, not assumed good"
        );
        let cleared =
            settle_receipt(&json!({ "success": true, "transaction": "0xabc", "payer": "0xpayer" }))
                .expect("an explicit success is a receipt");
        assert_eq!(cleared.tx_hash, "0xabc");
        assert_eq!(cleared.payer, "0xpayer");
        assert_eq!(cleared.amount, PRICE_ATOMIC_USDC);
    }

    /// The same reading rule on the other call: absent is not valid (AGENTS
    /// rule 8), so a silent or malformed answer never passes as approval.
    #[test]
    fn only_an_explicit_is_valid_is_a_verified_payment() {
        assert!(
            !verify_outcome(&json!({})).valid,
            "a body with no isValid is not approval"
        );
        let refused = verify_outcome(&json!({ "isValid": false, "invalidReason": "expired" }));
        assert!(!refused.valid);
        assert_eq!(refused.reason, Some("expired".to_owned()));
        let good = verify_outcome(&json!({ "isValid": true, "payer": "0xpayer" }));
        assert!(good.valid);
        assert_eq!(good.payer, Some("0xpayer".to_owned()));
    }
}
