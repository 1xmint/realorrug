// SPDX-License-Identifier: Apache-2.0
//! EIP-1559 transactions: built, encoded, decoded, hashed, and their signer
//! recovered.
//!
//! # Why by hand
//!
//! The payout builds exactly one kind of transaction: type 2, one recipient, an
//! empty access list. RLP for that is a page, and the one thing that can prove
//! it is a transaction the network accepted: `tests/claim_as_mainnet_signed_it.rs`
//! rebuilds the captured escrow claim from its fields and gets mainnet's hash
//! and mainnet's sender, to the byte. A general Ethereum library would be tens
//! of thousands of lines in a process that moves money.
//!
//! # Why decode as well as encode
//!
//! Turnkey holds the key and returns signed bytes. It is trusted to keep the
//! key, not to sign the right thing: [`decode_signed`] reads those bytes back
//! into fields the payout compares with what it asked for, and [`recover`]
//! names the account that signed them. The decoder is strict -- no
//! non-canonical lengths, no leading zeros, nothing after the transaction --
//! so two different byte strings never decode to the same fields and the
//! comparison covers every byte that is sent.

use k256::FieldBytes;
use k256::ecdsa::signature::hazmat::PrehashVerifier as _;
use k256::ecdsa::{RecoveryId, Signature, VerifyingKey};
use realorrug_robinhood::{Address, Hash32};
use sha3::{Digest as _, Keccak256};

/// The EIP-2718 type byte of an EIP-1559 transaction.
pub const TYPE_EIP1559: u8 = 0x02;

/// Why bytes are not a transaction this crate accepts.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TxError {
    /// Not type 2.
    #[error("not an EIP-1559 transaction")]
    NotEip1559,
    /// The bytes end before an item does, or continue after the transaction.
    #[error("truncated or trailing bytes")]
    Length,
    /// An encoding RLP allows only one way to write, written another way.
    #[error("non-canonical RLP")]
    NonCanonical,
    /// An item that should be a list is a string, or the other way round.
    #[error("wrong RLP shape: {0}")]
    Shape(&'static str),
    /// An integer wider than its field.
    #[error("integer too wide: {0}")]
    Wide(&'static str),
    /// A non-empty access list, which the payout never sends.
    #[error("the access list is not empty")]
    AccessList,
    /// A signature that is malformed, not low-s, or recovers to nothing.
    #[error("bad signature: {0}")]
    Signature(&'static str),
}

/// Keccak-256.
#[must_use]
pub fn keccak(bytes: &[u8]) -> [u8; 32] {
    Keccak256::digest(bytes).into()
}

/// An unsigned EIP-1559 transaction with an empty access list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Eip1559 {
    /// The chain it is valid on.
    pub chain_id: u64,
    /// The sender's nonce.
    pub nonce: u64,
    /// The tip, in wei per gas.
    pub max_priority_fee_per_gas: u128,
    /// The most paid per gas, tip included.
    pub max_fee_per_gas: u128,
    /// The gas limit.
    pub gas_limit: u64,
    /// The recipient. Never absent: the payout deploys nothing.
    pub to: Address,
    /// Wei sent.
    pub value: u128,
    /// Call data.
    pub data: Vec<u8>,
}

/// A signed EIP-1559 transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Signed {
    /// What was signed.
    pub tx: Eip1559,
    /// Whether the signature's point has an odd y.
    pub y_parity: bool,
    /// The signature's r, big-endian.
    pub r: [u8; 32],
    /// The signature's s, big-endian.
    pub s: [u8; 32],
}

impl Eip1559 {
    /// The nine fields' RLP, concatenated, without the list header.
    fn payload(&self) -> Vec<u8> {
        let mut out = Vec::new();
        uint(&mut out, self.chain_id.into());
        uint(&mut out, self.nonce.into());
        uint(&mut out, self.max_priority_fee_per_gas);
        uint(&mut out, self.max_fee_per_gas);
        uint(&mut out, self.gas_limit.into());
        string(&mut out, &self.to.0);
        uint(&mut out, self.value);
        string(&mut out, &self.data);
        // The access list: always empty, always present.
        list(&mut out, &[]);
        out
    }

    /// `0x02 ‖ rlp([chain_id, nonce, tip, max_fee, gas, to, value, data, []])`:
    /// the bytes whose Keccak is signed, and what Turnkey is asked to sign.
    #[must_use]
    pub fn unsigned(&self) -> Vec<u8> {
        let mut out = vec![TYPE_EIP1559];
        list(&mut out, &self.payload());
        out
    }

    /// The hash the signature is over.
    #[must_use]
    pub fn signing_hash(&self) -> [u8; 32] {
        keccak(&self.unsigned())
    }
}

impl Signed {
    /// The bytes sent to the network.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut payload = self.tx.payload();
        uint(&mut payload, u128::from(self.y_parity));
        string(&mut payload, trim(&self.r));
        string(&mut payload, trim(&self.s));
        let mut out = vec![TYPE_EIP1559];
        list(&mut out, &payload);
        out
    }

    /// The transaction hash: Keccak of the bytes sent.
    #[must_use]
    pub fn hash(&self) -> Hash32 {
        Hash32(keccak(&self.encode()))
    }
}

/// Big-endian bytes with leading zeros removed; zero is empty.
fn trim(bytes: &[u8]) -> &[u8] {
    let first = bytes.iter().position(|&b| b != 0).unwrap_or(bytes.len());
    &bytes[first..]
}

/// An RLP length header. `offset` is 0x80 for a string, 0xc0 for a list.
#[allow(
    clippy::cast_possible_truncation,
    reason = "a usize is at most eight bytes, so the header byte is at most 0xbf or 0xff"
)]
fn header(out: &mut Vec<u8>, offset: u8, len: usize) {
    if let Ok(short) = u8::try_from(len)
        && short <= 55
    {
        out.push(offset + short);
    } else {
        let be = (len as u64).to_be_bytes();
        let be = trim(&be);
        // At most eight length bytes, so the sum is at most 0xbf or 0xff.
        out.push(offset + 55 + be.len() as u8);
        out.extend_from_slice(be);
    }
}

/// An RLP string.
fn string(out: &mut Vec<u8>, bytes: &[u8]) {
    match bytes {
        [one] if *one < 0x80 => out.push(*one),
        _ => {
            header(out, 0x80, bytes.len());
            out.extend_from_slice(bytes);
        }
    }
}

/// An RLP integer: the string of its minimal big-endian bytes.
fn uint(out: &mut Vec<u8>, value: u128) {
    string(out, trim(&value.to_be_bytes()));
}

/// An RLP list around an already-encoded payload.
fn list(out: &mut Vec<u8>, payload: &[u8]) {
    header(out, 0xc0, payload.len());
    out.extend_from_slice(payload);
}

/// One decoded RLP item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Item<'a> {
    /// A string's bytes.
    Bytes(&'a [u8]),
    /// A list's payload, still encoded.
    List(&'a [u8]),
}

/// The first `len` bytes, and the rest.
fn take(from: &[u8], len: usize) -> Result<(&[u8], &[u8]), TxError> {
    if from.len() < len {
        Err(TxError::Length)
    } else {
        Ok(from.split_at(len))
    }
}

/// A long-form length of `width` bytes, and what follows it.
fn long_length(from: &[u8], width: usize) -> Result<(usize, &[u8]), TxError> {
    let (be, after) = take(from, width)?;
    // A long length has no leading zero byte, and is long for a reason.
    if be.first() == Some(&0) {
        return Err(TxError::NonCanonical);
    }
    let len = be
        .iter()
        .try_fold(0usize, |acc, &b| {
            acc.checked_mul(256).map(|v| v + usize::from(b))
        })
        .ok_or(TxError::Length)?;
    if len <= 55 {
        return Err(TxError::NonCanonical);
    }
    Ok((len, after))
}

/// The first item in `input`, and what follows it.
fn item(input: &[u8]) -> Result<(Item<'_>, &[u8]), TxError> {
    let (&first, rest) = input.split_first().ok_or(TxError::Length)?;
    match first {
        0x00..=0x7f => Ok((Item::Bytes(&input[..1]), rest)),
        0x80..=0xb7 => {
            let (bytes, after) = take(rest, usize::from(first - 0x80))?;
            // A single byte below 0x80 is its own encoding.
            if let [one] = bytes
                && *one < 0x80
            {
                return Err(TxError::NonCanonical);
            }
            Ok((Item::Bytes(bytes), after))
        }
        0xb8..=0xbf => {
            let (len, after) = long_length(rest, usize::from(first - 0xb7))?;
            let (bytes, after) = take(after, len)?;
            Ok((Item::Bytes(bytes), after))
        }
        0xc0..=0xf7 => {
            let (payload, after) = take(rest, usize::from(first - 0xc0))?;
            Ok((Item::List(payload), after))
        }
        0xf8..=0xff => {
            let (len, after) = long_length(rest, usize::from(first - 0xf7))?;
            let (payload, after) = take(after, len)?;
            Ok((Item::List(payload), after))
        }
    }
}

/// Every item in a list's payload.
fn items(mut payload: &[u8]) -> Result<Vec<Item<'_>>, TxError> {
    let mut out = Vec::new();
    while !payload.is_empty() {
        let (one, rest) = item(payload)?;
        out.push(one);
        payload = rest;
    }
    Ok(out)
}

fn bytes<'a>(it: Item<'a>, name: &'static str) -> Result<&'a [u8], TxError> {
    match it {
        Item::Bytes(b) => Ok(b),
        Item::List(_) => Err(TxError::Shape(name)),
    }
}

/// An integer of at most `width` bytes, minimally encoded.
fn integer(it: Item<'_>, name: &'static str, width: usize) -> Result<u128, TxError> {
    let b = bytes(it, name)?;
    if b.first() == Some(&0) {
        return Err(TxError::NonCanonical);
    }
    if b.len() > width {
        return Err(TxError::Wide(name));
    }
    // Padded into a u128's bytes rather than folded with shifts, so there is
    // no `|` that an `^` computes identically.
    let mut be = [0u8; 16];
    be[16 - b.len()..].copy_from_slice(b);
    Ok(u128::from_be_bytes(be))
}

fn integer_u64(it: Item<'_>, name: &'static str) -> Result<u64, TxError> {
    integer(it, name, 8).and_then(|v| u64::try_from(v).map_err(|_| TxError::Wide(name)))
}

fn word(it: Item<'_>, name: &'static str) -> Result<[u8; 32], TxError> {
    let b = bytes(it, name)?;
    if b.first() == Some(&0) {
        return Err(TxError::NonCanonical);
    }
    if b.len() > 32 {
        return Err(TxError::Wide(name));
    }
    let mut out = [0u8; 32];
    out[32 - b.len()..].copy_from_slice(b);
    Ok(out)
}

/// A signed EIP-1559 transaction from the bytes a signer returned.
///
/// # Errors
///
/// A [`TxError`] for anything but exactly one canonical type-2 transaction with
/// a recipient, an empty access list, and a y-parity of 0 or 1.
pub fn decode_signed(raw: &[u8]) -> Result<Signed, TxError> {
    let (&kind, body) = raw.split_first().ok_or(TxError::Length)?;
    if kind != TYPE_EIP1559 {
        return Err(TxError::NotEip1559);
    }
    let (outer, trailing) = item(body)?;
    if !trailing.is_empty() {
        return Err(TxError::Length);
    }
    let Item::List(payload) = outer else {
        return Err(TxError::Shape("transaction"));
    };
    let fields = items(payload)?;
    let [
        chain_id,
        nonce,
        tip,
        max_fee,
        gas,
        to,
        value,
        data,
        access,
        y_parity,
        r,
        s,
    ] = fields.as_slice()
    else {
        return Err(TxError::Shape("twelve fields"));
    };
    match access {
        Item::List([]) => {}
        Item::List(_) => return Err(TxError::AccessList),
        Item::Bytes(_) => return Err(TxError::Shape("access list")),
    }
    let to = <[u8; 20]>::try_from(bytes(*to, "to")?).map_err(|_| TxError::Shape("to"))?;
    let y_parity = match integer(*y_parity, "y parity", 1)? {
        0 => false,
        1 => true,
        _ => return Err(TxError::Signature("y parity is not 0 or 1")),
    };
    Ok(Signed {
        tx: Eip1559 {
            chain_id: integer_u64(*chain_id, "chain id")?,
            nonce: integer_u64(*nonce, "nonce")?,
            max_priority_fee_per_gas: integer(*tip, "tip", 16)?,
            max_fee_per_gas: integer(*max_fee, "max fee", 16)?,
            gas_limit: integer_u64(*gas, "gas")?,
            to: Address(to),
            value: integer(*value, "value", 16)?,
            data: bytes(*data, "data")?.to_vec(),
        },
        y_parity,
        r: word(*r, "r")?,
        s: word(*s, "s")?,
    })
}

/// The address a public key controls: the last 20 bytes of the Keccak of its
/// uncompressed point, without the `0x04` prefix.
#[must_use]
pub fn address_of(key: &VerifyingKey) -> Address {
    let point = key.to_sec1_point(false);
    let hash = keccak(&point.as_bytes()[1..]);
    let mut out = [0u8; 20];
    out.copy_from_slice(&hash[12..]);
    Address(out)
}

/// An address in EIP-55's mixed case: each hex letter is upper case where the
/// matching nibble of the Keccak of the lowercase hex is 8 or more.
///
/// Turnkey's `signWith` needs this form. It matches addresses exactly as it
/// stores them, and the lowercase form [`Address`] prints was refused on the
/// first setup proof: "Could not find any resource to sign with. Addresses are
/// case sensitive."
#[must_use]
pub fn checksummed(address: &Address) -> String {
    let lower = address.to_string();
    let digits = &lower[2..];
    let hash = keccak(digits.as_bytes());
    let mut out = String::from("0x");
    for (i, c) in digits.chars().enumerate() {
        let nibble = (hash[i / 2] >> if i % 2 == 0 { 4 } else { 0 }) & 0x0f;
        out.push(if nibble >= 8 {
            c.to_ascii_uppercase()
        } else {
            c
        });
    }
    out
}

/// The account that signed a transaction.
///
/// Refuses a high-s signature, which Ethereum has rejected since EIP-2, and
/// verifies the signature under the recovered key rather than trusting the
/// recovery alone: recovery computes *a* key from any r and s, and the
/// verification is what says that key signed this hash.
///
/// # Errors
///
/// [`TxError::Signature`] with the reason.
pub fn recover(signed: &Signed) -> Result<Address, TxError> {
    let signature = Signature::from_scalars(FieldBytes::from(signed.r), FieldBytes::from(signed.s))
        .map_err(|_| TxError::Signature("r or s is zero or not below the curve order"))?;
    if signature.normalize_s() != signature {
        return Err(TxError::Signature("s is high"));
    }
    let hash = signed.tx.signing_hash();
    let key = VerifyingKey::recover_from_prehash(
        &hash,
        &signature,
        RecoveryId::new(signed.y_parity, false),
    )
    .map_err(|_| TxError::Signature("recovers to no key"))?;
    key.verify_prehash(&hash, &signature)
        .map_err(|_| TxError::Signature("does not verify under the recovered key"))?;
    Ok(address_of(&key))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enc_string(b: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        string(&mut out, b);
        out
    }

    fn enc_uint(v: u128) -> Vec<u8> {
        let mut out = Vec::new();
        uint(&mut out, v);
        out
    }

    #[test]
    fn checksummed_matches_eip55s_own_examples_and_the_turnkey_wallet() {
        // EIP-55's test vectors, all-caps and all-lowercase included, and the
        // payout wallet as Turnkey's dashboard shows it. Re-apply by testing
        // the nibble against 7, or by taking the high nibble for every digit:
        // these fail.
        for expected in [
            "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed",
            "0xfB6916095ca1df60bB79Ce92cE3Ea74c37c5d359",
            "0xdbF03B407c01E7cD3CBea99509d93f8DDDC8C6FB",
            "0xD1220A0cf47c7B9Be7A2E6BA89F429762e7b9aDb",
            "0x52908400098527886E0F7030069857D2E4169EE7",
            "0xde709f2102306220921060314715629080e2fb77",
            "0xD66578AC5fD7E36F8427C61Edb7a2cbFA4c75793",
        ] {
            let address: Address = expected.parse().expect("an address");
            assert_eq!(checksummed(&address), expected);
        }
    }

    #[test]
    fn rlp_matches_the_specifications_own_examples() {
        // The examples in the Ethereum wiki's RLP page and the yellow paper's
        // appendix B. Re-apply by emitting `0x81 0x0f` for 15, or `0x00` for
        // zero: the integer assertions fail.
        assert_eq!(enc_string(b""), [0x80]);
        assert_eq!(enc_string(b"dog"), [0x83, b'd', b'o', b'g']);
        assert_eq!(enc_string(&[0x00]), [0x00]);
        assert_eq!(enc_string(&[0x7f]), [0x7f]);
        assert_eq!(enc_string(&[0x80]), [0x81, 0x80]);
        assert_eq!(enc_uint(0), [0x80]);
        assert_eq!(enc_uint(15), [0x0f]);
        assert_eq!(enc_uint(1024), [0x82, 0x04, 0x00]);
        let mut empty = Vec::new();
        list(&mut empty, &[]);
        assert_eq!(empty, [0xc0]);
        let mut cat_dog = Vec::new();
        list(
            &mut cat_dog,
            &[enc_string(b"cat"), enc_string(b"dog")].concat(),
        );
        assert_eq!(
            cat_dog,
            [0xc8, 0x83, b'c', b'a', b't', 0x83, b'd', b'o', b'g']
        );
        // 56 bytes is the first string with a long header: `0xb8 0x38`.
        let long = [b'x'; 56];
        let got = enc_string(&long);
        assert_eq!(&got[..2], &[0xb8, 0x38]);
        assert_eq!(got.len(), 58);
        // And 55 is the last short one.
        assert_eq!(enc_string(&[b'x'; 55])[0], 0xb7);
    }

    #[test]
    fn a_decoded_item_is_the_encoded_one_and_a_second_spelling_is_refused() {
        // Strictness is what makes "the signed bytes decode to our fields" a
        // statement about every byte. Re-apply by accepting `0x81 0x05`: the
        // first refusal passes and two byte strings mean the same integer.
        let long = enc_string(&[b'x'; 56]);
        assert_eq!(item(&long), Ok((Item::Bytes(&[b'x'; 56]), &[][..])));
        assert_eq!(item(&[0x81, 0x05]), Err(TxError::NonCanonical));
        // 0x80 itself is not its own encoding, so its one-byte string is
        // canonical. Re-apply `<=` for `<` in the check: this is refused.
        assert_eq!(item(&[0x81, 0x80]), Ok((Item::Bytes(&[0x80]), &[][..])));
        // Two length bytes, for a string and for a list. 0xb9 and 0xf9 name a
        // two-byte length; re-apply `/` for `-` and they read one byte.
        let mut wide = vec![0xb9, 0x01, 0x00];
        wide.extend_from_slice(&[b'x'; 256]);
        assert_eq!(item(&wide), Ok((Item::Bytes(&[b'x'; 256]), &[][..])));
        wide[0] = 0xf9;
        assert_eq!(item(&wide), Ok((Item::List(&[b'x'; 256]), &[][..])));
        // An r or s of 33 bytes is too wide; 32 is not.
        assert_eq!(word(Item::Bytes(&[1; 33]), "r"), Err(TxError::Wide("r")));
        assert_eq!(word(Item::Bytes(&[1; 32]), "r"), Ok([1; 32]));
        assert_eq!(word(Item::Bytes(&[1; 31]), "r").map(|w| w[0]), Ok(0));
        // A long header for a short string.
        assert_eq!(
            item(&[0xb8, 0x05, 1, 2, 3, 4, 5]),
            Err(TxError::NonCanonical)
        );
        // A long length with a leading zero byte.
        let mut padded = vec![0xb9, 0x00, 0x38];
        padded.extend_from_slice(&[b'x'; 56]);
        assert_eq!(item(&padded), Err(TxError::NonCanonical));
        // Truncated.
        assert_eq!(item(&[0x83, b'd', b'o']), Err(TxError::Length));
        assert_eq!(item(&[]), Err(TxError::Length));
        assert_eq!(item(&[0xb8]), Err(TxError::Length));
        // An integer with a leading zero, and one too wide for its field.
        assert_eq!(
            integer(Item::Bytes(&[0, 1]), "n", 8),
            Err(TxError::NonCanonical)
        );
        assert_eq!(
            integer(Item::Bytes(&[1; 9]), "n", 8),
            Err(TxError::Wide("n"))
        );
        assert_eq!(integer(Item::Bytes(&[]), "n", 8), Ok(0));
        assert_eq!(integer(Item::Bytes(&[0x12, 0x37]), "n", 8), Ok(0x1237));
    }

    #[test]
    fn a_signed_transaction_round_trips_and_its_signer_is_recovered() {
        // A local key standing in for Turnkey's. The same path the payout
        // takes with Turnkey's answer: sign the hash, encode, decode, recover.
        let key = k256::ecdsa::SigningKey::from_bytes(&FieldBytes::from([7u8; 32])).expect("a key");
        let tx = Eip1559 {
            chain_id: 4663,
            nonce: 1_000_000,
            max_priority_fee_per_gas: 0,
            max_fee_per_gas: 205_817_992,
            gas_limit: 60_000,
            to: Address([0xd3; 20]),
            value: u128::from(u64::MAX) + 1,
            data: vec![0xab; 80],
        };
        let (signature, id) = key.sign_prehash_recoverable(&tx.signing_hash());
        let (r, s) = signature.split_bytes();
        let signed = Signed {
            tx: tx.clone(),
            y_parity: id.is_y_odd(),
            r: r.into(),
            s: s.into(),
        };
        let raw = signed.encode();
        assert_eq!(decode_signed(&raw), Ok(signed.clone()));
        assert_eq!(recover(&signed), Ok(address_of(key.verifying_key())));

        // The other parity recovers a different key, which then does not
        // verify, or verifies as a different address; either way not ours.
        let flipped = Signed {
            y_parity: !signed.y_parity,
            ..signed.clone()
        };
        assert_ne!(recover(&flipped), Ok(address_of(key.verifying_key())));

        // The same signature over different fields names somebody else.
        let mut other = signed.clone();
        other.tx.value -= 1;
        assert_ne!(recover(&other), Ok(address_of(key.verifying_key())));

        // A trailing byte, a legacy type, and a non-empty access list.
        let mut trailing = raw.clone();
        trailing.push(0);
        assert_eq!(decode_signed(&trailing), Err(TxError::Length));
        let mut legacy = raw.clone();
        legacy[0] = 0x01;
        assert_eq!(decode_signed(&legacy), Err(TxError::NotEip1559));
    }

    #[test]
    fn a_high_s_signature_is_refused_even_though_it_recovers() {
        use k256::elliptic_curve::ff::PrimeField as _;
        // Both (r, s) and (r, n - s) verify under plain ECDSA; Ethereum accepts
        // only the low one. Re-apply by deleting the check: the high twin
        // recovers to the same key and this passes where it must not.
        let key = k256::ecdsa::SigningKey::from_bytes(&FieldBytes::from([9u8; 32])).expect("a key");
        let tx = Eip1559 {
            chain_id: 4663,
            nonce: 0,
            max_priority_fee_per_gas: 0,
            max_fee_per_gas: 1,
            gas_limit: 21_000,
            to: Address([1; 20]),
            value: 1,
            data: Vec::new(),
        };
        let (signature, id) = key.sign_prehash_recoverable(&tx.signing_hash());
        let high = Signature::from_scalars(signature.r().to_repr(), (-*signature.s()).to_repr())
            .expect("the twin");
        let (r, s) = high.split_bytes();
        let signed = Signed {
            tx,
            y_parity: !id.is_y_odd(),
            r: r.into(),
            s: s.into(),
        };
        assert_eq!(recover(&signed), Err(TxError::Signature("s is high")));
    }
}
