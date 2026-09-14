// SPDX-License-Identifier: Apache-2.0
//! Turnkey: where the payout key lives, and the one call that uses it.
//!
//! ADR 0025. The payout wallet's key is held by Turnkey and cannot be exported.
//! What the box holds is an **API key**: a secp256k1 key that signs each
//! request to Turnkey's API, which then applies the organisation's policy and,
//! if it allows the request, signs the transaction with the wallet key. The API
//! key can ask for nothing the policy does not allow, and revoking it in the
//! dashboard ends even that.
//!
//! # The stamp
//!
//! Every request carries an `X-Stamp` header: base64url, without padding, of
//! `{"publicKey", "scheme", "signature"}`, where the signature is ECDSA over
//! the SHA-256 of the exact request body, DER-encoded, then hex. That is the
//! shape of Turnkey's own Rust stamper (`tkhq/rust-sdk`,
//! `api_key_stamper/src/lib.rs`), written here in thirty lines rather than
//! depended on, because that crate brings a second copy of the curve and
//! digest crates into a process that moves money.
//!
//! # Trusted to hold the key, not to sign the right thing
//!
//! [`Turnkey::sign_transaction`] returns whatever bytes Turnkey sent. The
//! caller, `crate::sign_checked`, decodes them, compares the fields with the
//! ones asked for, and recovers the signer, before anything is sent.

use std::path::Path;
use std::time::Duration;

use base64::Engine as _;
use k256::FieldBytes;
use k256::ecdsa::signature::Signer as _;
use k256::ecdsa::{Signature, SigningKey};
use realorrug_robinhood::Address;
use serde_json::{Value, json};

use crate::tx::Eip1559;

/// Turnkey's API. Not configurable in production: a variable naming a different
/// signer would be a way to send the request, stamp and all, somewhere else.
pub const API: &str = "https://api.turnkey.com";

/// The stamp scheme for a secp256k1 API key.
pub const SCHEME: &str = "SIGNATURE_SCHEME_TK_API_SECP256K1";

/// The activity that signs a transaction.
pub const SIGN_TRANSACTION: &str = "ACTIVITY_TYPE_SIGN_TRANSACTION_V2";

/// How long one request to Turnkey may take before it counts as failed. A hung
/// call holds the payout lock, and the next run should say so rather than wait.
const TIMEOUT: Duration = Duration::from_secs(30);

/// Why the API key file was not accepted.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum KeyError {
    /// The file could not be read.
    #[error("the API key file could not be read: {0}")]
    Read(String),
    /// Not `0x` and 64 hex digits.
    #[error("the API key file is not 0x and 64 hex digits")]
    Malformed,
    /// Zero, or not below the curve order: not a private key.
    #[error("the API key is not a valid secp256k1 private key")]
    OutOfRange,
    /// Someone other than its owner can read it.
    #[error("the API key file is readable by group or others (mode {mode:o}); it must be 0400")]
    Readable {
        /// The permission bits found.
        mode: u32,
    },
    /// The key's public half is not the one configured.
    #[error("the API key's public key is {derived}, not the configured TURNKEY_API_PUBLIC_KEY")]
    WrongPublicKey {
        /// What the file's key derives.
        derived: String,
    },
}

/// A Turnkey API key: the private key that stamps requests, and its public half.
pub struct ApiKey {
    key: SigningKey,
    public: String,
}

impl std::fmt::Debug for ApiKey {
    /// The public half only. A private key never reaches a log line.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApiKey")
            .field("public", &self.public)
            .finish_non_exhaustive()
    }
}

/// Lowercase hex, no prefix.
#[must_use]
pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

impl ApiKey {
    /// A key from its file's text, checked against the configured public key.
    ///
    /// The public-key comparison is what catches the wrong file: a key for a
    /// different Turnkey user would stamp requests the policy then refuses, or
    /// -- worse -- a key for the root user would stamp requests it allows.
    ///
    /// # Errors
    ///
    /// [`KeyError::Malformed`], [`KeyError::OutOfRange`] or
    /// [`KeyError::WrongPublicKey`].
    pub fn from_text(text: &str, expected_public: &str) -> Result<Self, KeyError> {
        let digits = text
            .trim()
            .strip_prefix("0x")
            .filter(|d| d.len() == 64 && d.bytes().all(|c| c.is_ascii_hexdigit()))
            .ok_or(KeyError::Malformed)?;
        let mut secret = [0u8; 32];
        for (i, byte) in secret.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&digits[2 * i..2 * i + 2], 16)
                .map_err(|_| KeyError::Malformed)?;
        }
        let key =
            SigningKey::from_bytes(&FieldBytes::from(secret)).map_err(|_| KeyError::OutOfRange)?;
        let public = hex(key.verifying_key().to_sec1_point(true).as_bytes());
        if public != expected_public.trim().to_ascii_lowercase() {
            return Err(KeyError::WrongPublicKey { derived: public });
        }
        Ok(Self { key, public })
    }

    /// The compressed public key, hex: what Turnkey knows this key by.
    #[must_use]
    pub fn public_hex(&self) -> &str {
        &self.public
    }

    /// The `X-Stamp` header value for a request body.
    #[must_use]
    pub fn stamp(&self, body: &str) -> String {
        // SHA-256 over the body, as `k256`'s `Signer` does for secp256k1, with
        // s normalised low.
        let signature: Signature = self.key.sign(body.as_bytes());
        let stamp = json!({
            "publicKey": self.public,
            "scheme": SCHEME,
            "signature": hex(&der(&signature)),
        });
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(stamp.to_string())
    }
}

/// An ECDSA signature as ASN.1 DER: `SEQUENCE { INTEGER r, INTEGER s }`.
///
/// Each integer is minimal big-endian with a zero byte in front when its top
/// bit is set, so it does not read as negative. r and s are nonzero and at most
/// 32 bytes, so every length fits one byte and there is one way to write it.
/// Written out rather than enabling `k256`'s `pkcs8` feature, which brings three
/// crates for these twelve lines; the test parses it with that feature on.
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    reason = "an integer is at most 33 bytes and the sequence at most 70"
)]
pub fn der(signature: &Signature) -> Vec<u8> {
    let (r, s) = signature.split_bytes();
    let integer = |be: &[u8]| {
        let first = be.iter().position(|&b| b != 0).unwrap_or(be.len() - 1);
        let digits = &be[first..];
        let pad = digits[0] & 0x80 != 0;
        let mut out = vec![0x02, (digits.len() + usize::from(pad)) as u8];
        if pad {
            out.push(0);
        }
        out.extend_from_slice(digits);
        out
    };
    let body = [integer(&r), integer(&s)].concat();
    let mut out = vec![0x30, body.len() as u8];
    out.extend_from_slice(&body);
    out
}

/// Reads the API key file, refusing one that others can read.
///
/// # Errors
///
/// Any [`KeyError`].
pub fn load_api_key(path: &Path, expected_public: &str) -> Result<ApiKey, KeyError> {
    let meta =
        std::fs::metadata(path).map_err(|e| KeyError::Read(format!("{}: {e}", path.display())))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = meta.permissions().mode() & 0o777;
        if mode & 0o077 != 0 {
            return Err(KeyError::Readable { mode });
        }
    }
    #[cfg(not(unix))]
    let _ = meta;
    let text = std::fs::read_to_string(path)
        .map_err(|e| KeyError::Read(format!("{}: {e}", path.display())))?;
    ApiKey::from_text(&text, expected_public)
}

/// A Turnkey organisation, one wallet account in it, and the API key that asks.
#[derive(Debug)]
pub struct Turnkey {
    base: String,
    organization: String,
    sign_with: Address,
    key: ApiKey,
}

/// The time a request is stamped with, in milliseconds since the epoch.
fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis())
}

impl Turnkey {
    /// A client. `base` is [`API`] in production and a loopback server in
    /// tests.
    #[must_use]
    pub fn new(
        base: impl Into<String>,
        organization: impl Into<String>,
        sign_with: Address,
        key: ApiKey,
    ) -> Self {
        Self {
            base: base.into(),
            organization: organization.into(),
            sign_with,
            key,
        }
    }

    /// The account Turnkey signs for.
    #[must_use]
    pub const fn account(&self) -> Address {
        self.sign_with
    }

    /// Posts a stamped JSON body and returns the answer.
    ///
    /// An HTTP error is read rather than raised, because Turnkey says why it
    /// refused in the body, and "denied by policy" is the answer an operator
    /// needs to see.
    fn post(&self, path: &str, body: &Value) -> Result<Value, String> {
        let text = body.to_string();
        let mut response = ureq::post(&format!("{}{path}", self.base))
            .config()
            .http_status_as_error(false)
            .timeout_global(Some(TIMEOUT))
            .build()
            .header("X-Stamp", self.key.stamp(&text))
            .content_type("application/json")
            .send(text)
            .map_err(|e| format!("turnkey {path}: {e}"))?;
        let status = response.status();
        let answer = response
            .body_mut()
            .read_to_string()
            .map_err(|e| format!("turnkey {path}: {e}"))?;
        if !status.is_success() {
            return Err(format!(
                "turnkey {path}: HTTP {}: {answer}",
                status.as_u16()
            ));
        }
        serde_json::from_str(&answer).map_err(|e| format!("turnkey {path}: not json: {e}"))
    }

    /// Who the API key is, to Turnkey. The setup proof's first request.
    ///
    /// # Errors
    ///
    /// Turnkey's refusal or the transport's.
    pub fn whoami(&self) -> Result<Value, String> {
        self.post(
            "/public/v1/query/whoami",
            &json!({ "organizationId": self.organization }),
        )
    }

    /// The request body that asks Turnkey to sign `tx`.
    #[must_use]
    pub fn sign_body(&self, tx: &Eip1559, timestamp_ms: u128) -> Value {
        json!({
            "type": SIGN_TRANSACTION,
            "timestampMs": timestamp_ms.to_string(),
            "organizationId": self.organization,
            "parameters": {
                "signWith": self.sign_with.to_string(),
                "type": "TRANSACTION_TYPE_ETHEREUM",
                // Hex without `0x`, as Turnkey's viem adapter sends it.
                "unsignedTransaction": hex(&tx.unsigned()),
            },
        })
    }

    /// Asks Turnkey to sign `tx` and returns the signed bytes, unchecked.
    ///
    /// # Errors
    ///
    /// Turnkey's words when the activity is not completed -- denied by policy,
    /// failed, or waiting on a consensus nobody will give -- or the transport's.
    /// Never retried here: a refusal is an answer, and asking again in a loop
    /// is how a policy gets worn down into an alert nobody reads.
    pub fn sign_transaction(&self, tx: &Eip1559) -> Result<Vec<u8>, String> {
        let answer = self.post(
            "/public/v1/submit/sign_transaction",
            &self.sign_body(tx, now_ms()),
        )?;
        signed_from(&answer)
    }
}

/// The signed transaction in a `sign_transaction` answer, if it completed.
///
/// # Errors
///
/// The activity's status and whatever Turnkey said about it, when it is not
/// `ACTIVITY_STATUS_COMPLETED` or carries no signed transaction.
pub fn signed_from(answer: &Value) -> Result<Vec<u8>, String> {
    let activity = &answer["activity"];
    let status = activity["status"].as_str().unwrap_or("no status");
    if status != "ACTIVITY_STATUS_COMPLETED" {
        return Err(format!(
            "turnkey activity {status}: {}",
            activity
                .get("failure")
                .map_or_else(|| answer.to_string(), Value::to_string)
        ));
    }
    let text = activity["result"]["signTransactionResult"]["signedTransaction"]
        .as_str()
        .ok_or("turnkey completed with no signed transaction")?;
    let digits = text.strip_prefix("0x").unwrap_or(text);
    realorrug_robinhood::hex_bytes(&format!("0x{digits}"))
        .map_err(|e| format!("turnkey's signed transaction: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use k256::ecdsa::signature::Verifier as _;

    /// A fixed API key for tests, never used anywhere else.
    const SECRET: &str = "0x1111111111111111111111111111111111111111111111111111111111111111";

    fn public_of(secret: &str) -> String {
        let bytes = realorrug_robinhood::hex_bytes(secret).expect("hex");
        let key = SigningKey::from_slice(&bytes).expect("a key");
        hex(key.verifying_key().to_sec1_point(true).as_bytes())
    }

    #[test]
    fn the_stamp_is_the_three_fields_turnkey_reads_signed_over_the_bodys_sha256() {
        // Re-apply by signing the Keccak of the body: the verification below
        // fails. Re-apply by using padded base64: the no-padding decoder
        // refuses the `=` whenever the stamp's length is not a multiple of
        // three, and the `=` assertion says so directly.
        let key = ApiKey::from_text(SECRET, &public_of(SECRET)).expect("loads");
        let body = r#"{"organizationId":"org","type":"ACTIVITY_TYPE_SIGN_TRANSACTION_V2"}"#;
        let header = key.stamp(body);
        assert!(!header.contains('='), "{header}");
        assert!(!header.contains('+') && !header.contains('/'), "{header}");
        let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(&header)
            .expect("base64url");
        let stamp: Value = serde_json::from_slice(&decoded).expect("json");
        assert_eq!(stamp.as_object().map(serde_json::Map::len), Some(3));
        assert_eq!(stamp["scheme"], SCHEME);
        assert_eq!(stamp["publicKey"], public_of(SECRET));
        // Compressed: 33 bytes, starting 02 or 03.
        let public = stamp["publicKey"].as_str().expect("text");
        assert_eq!(public.len(), 66);
        assert!(public.starts_with("02") || public.starts_with("03"));

        let der_bytes = realorrug_robinhood::hex_bytes(&format!(
            "0x{}",
            stamp["signature"].as_str().expect("text")
        ))
        .expect("hex");
        // Parsed by `k256`'s own DER decoder, enabled for tests only, so the
        // hand-written encoder is checked by something that is not itself.
        let signature = Signature::from_der(&der_bytes).expect("DER");
        let verifying =
            *SigningKey::from_slice(&realorrug_robinhood::hex_bytes(SECRET).expect("hex"))
                .expect("key")
                .verifying_key();
        verifying
            .verify(body.as_bytes(), &signature)
            .expect("verifies over the body");
        assert!(
            verifying.verify(b"another body", &signature).is_err(),
            "and over nothing else"
        );
    }

    #[test]
    fn der_pads_a_high_bit_and_strips_leading_zeros() {
        // r with its top bit set needs a zero in front; s with leading zero
        // bytes is written short. Both parsed back by the real decoder.
        let mut r = [0u8; 32];
        r[0] = 0x80;
        r[31] = 1;
        let mut s = [0u8; 32];
        s[30] = 0x01;
        s[31] = 0x02;
        let signature =
            Signature::from_scalars(FieldBytes::from(r), FieldBytes::from(s)).expect("valid");
        let bytes = der(&signature);
        assert_eq!(&bytes[..5], &[0x30, 0x27, 0x02, 0x21, 0x00]);
        assert_eq!(&bytes[bytes.len() - 4..], &[0x02, 0x02, 0x01, 0x02]);
        assert_eq!(Signature::from_der(&bytes).expect("parses"), signature);
    }

    #[test]
    fn the_api_key_loads_only_well_formed_in_range_and_matching_its_public_key() {
        let public = public_of(SECRET);
        assert!(ApiKey::from_text(&format!("{SECRET}\n"), &public).is_ok());
        // Case-insensitive public key, as a dashboard may print it.
        assert!(ApiKey::from_text(SECRET, &public.to_ascii_uppercase()).is_ok());

        let other = public_of("0x2222222222222222222222222222222222222222222222222222222222222222");
        assert_eq!(
            ApiKey::from_text(SECRET, &other).map(|k| k.public),
            Err(KeyError::WrongPublicKey {
                derived: public.clone()
            })
        );
        let zero = format!("0x{}", "0".repeat(64));
        assert_eq!(
            ApiKey::from_text(&zero, &public).map(|k| k.public),
            Err(KeyError::OutOfRange)
        );
        // The curve order itself is not a key; one below it is.
        let order = "0xfffffffffffffffffffffffffffffffebaaedce6af48a03bbfd25e8cd0364141";
        assert_eq!(
            ApiKey::from_text(order, &public).map(|k| k.public),
            Err(KeyError::OutOfRange)
        );
        let below = "0xfffffffffffffffffffffffffffffffebaaedce6af48a03bbfd25e8cd0364140";
        assert!(ApiKey::from_text(below, &public_of(below)).is_ok());
        for bad in [
            "1111111111111111111111111111111111111111111111111111111111111111",
            "0x111111111111111111111111111111111111111111111111111111111111111",
            "0x111111111111111111111111111111111111111111111111111111111111111g",
            "0x11111111111111111111111111111111111111111111111111111111111111111",
        ] {
            assert_eq!(
                ApiKey::from_text(bad, &public).map(|k| k.public),
                Err(KeyError::Malformed),
                "{bad}"
            );
        }
        // The private half never reaches a log line.
        let loaded = ApiKey::from_text(SECRET, &public).expect("loads");
        assert!(!format!("{loaded:?}").contains("1111111111"), "{loaded:?}");
    }

    #[cfg(unix)]
    #[test]
    fn an_api_key_file_others_can_read_is_refused() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir =
            std::env::temp_dir().join(format!("realorrug-turnkey-key-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("turnkey.key");
        std::fs::write(&path, SECRET).expect("write");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).expect("chmod");
        assert_eq!(
            load_api_key(&path, &public_of(SECRET)).map(|k| k.public),
            Err(KeyError::Readable { mode: 0o644 })
        );
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o400)).expect("chmod");
        assert!(load_api_key(&path, &public_of(SECRET)).is_ok());
    }

    #[test]
    fn a_missing_key_file_is_a_read_error() {
        let path = std::env::temp_dir().join("realorrug-turnkey-no-such-key");
        assert!(matches!(load_api_key(&path, "02"), Err(KeyError::Read(_))));
    }

    #[test]
    fn only_a_completed_activity_yields_bytes() {
        let done = json!({ "activity": { "status": "ACTIVITY_STATUS_COMPLETED",
            "result": { "signTransactionResult": { "signedTransaction": "02c0" } } } });
        assert_eq!(signed_from(&done), Ok(vec![0x02, 0xc0]));
        let prefixed = json!({ "activity": { "status": "ACTIVITY_STATUS_COMPLETED",
            "result": { "signTransactionResult": { "signedTransaction": "0x02c0" } } } });
        assert_eq!(signed_from(&prefixed), Ok(vec![0x02, 0xc0]));
        for status in [
            "ACTIVITY_STATUS_CONSENSUS_NEEDED",
            "ACTIVITY_STATUS_FAILED",
            "ACTIVITY_STATUS_REJECTED",
        ] {
            let answer =
                json!({ "activity": { "status": status, "failure": { "message": "policy" } } });
            let err = signed_from(&answer).expect_err("refused");
            assert!(err.contains(status) && err.contains("policy"), "{err}");
        }
        let empty = json!({ "activity": { "status": "ACTIVITY_STATUS_COMPLETED", "result": {} } });
        assert!(signed_from(&empty).is_err());
    }
}
