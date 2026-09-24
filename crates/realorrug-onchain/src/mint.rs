// SPDX-License-Identifier: Apache-2.0
//! Reading a mint account's authorities directly from its raw bytes.
//!
//! Split out of `dossier.rs` so `pumpfun_launch_check`'s check 4 (mint and
//! freeze authorities are both absent) can read the same fact `dossier`
//! reads for holder-share context, rather than keeping a second copy of the
//! byte offsets.

use realorrug_types::Address;

use crate::rpc::RpcError;

/// Reads a mint's two authorities directly from its raw account bytes.
///
/// The SPL Token and Token-2022 layouts share these two `COption<Pubkey>`
/// fields at the same offsets (mint authority's tag at byte 0, its address at
/// byte 4; freeze authority's tag at byte 46, its address at byte 50), so this
/// needs no program identity and no extension walk -- unlike
/// `realorrug_pumpfun::token::MintAccount::parse`, which refuses an
/// unmodelled extension entirely. Refusing an authority fact because of an
/// unrelated extension this reader has never seen would be the wrong trade:
/// the extension might change what a balance is worth, but it cannot move
/// where these two fields sit.
///
/// `MintAccount::parse` already reads `freeze_authority`, but discards
/// `mint_authority`'s address -- it only needs to know minting is possible,
/// not by whom. Repeating the two reads here is smaller than widening that
/// type for the one caller that needs the address.
///
/// # Errors
///
/// [`RpcError::Malformed`] when the account is shorter than a mint's base
/// layout, or an option tag is neither zero nor one.
///
/// # Panics
///
/// Never: the length check above guarantees every slice this function takes
/// (`try_into` on a fixed 4- or 32-byte window) is exactly the size it is
/// converted to.
pub fn mint_authorities(data: &[u8]) -> Result<(Option<Address>, Option<Address>), RpcError> {
    if data.len() < 82 {
        return Err(RpcError::Malformed(format!(
            "{} bytes, and a mint needs at least 82",
            data.len()
        )));
    }
    let option = |tag_at: usize, addr_at: usize| -> Result<Option<Address>, RpcError> {
        let tag = u32::from_le_bytes(data[tag_at..tag_at + 4].try_into().expect("checked above"));
        match tag {
            0 => Ok(None),
            1 => Ok(Some(Address::new(
                data[addr_at..addr_at + 32]
                    .try_into()
                    .expect("checked above"),
            ))),
            found => Err(RpcError::Malformed(format!(
                "mint authority option tag is {found}, which is neither none nor some"
            ))),
        }
    };
    let mint_authority = option(0, 4)?;
    let freeze_authority = option(46, 50)?;
    Ok((mint_authority, freeze_authority))
}

#[cfg(test)]
mod tests {
    use super::mint_authorities;
    use realorrug_types::Address;

    fn mint_bytes(mint_authority: Option<u8>, freeze_authority: Option<u8>) -> Vec<u8> {
        let mut data = vec![0u8; 82];
        if let Some(b) = mint_authority {
            data[0..4].copy_from_slice(&1u32.to_le_bytes());
            data[4..36].copy_from_slice(&[b; 32]);
        }
        if let Some(b) = freeze_authority {
            data[46..50].copy_from_slice(&1u32.to_le_bytes());
            data[50..82].copy_from_slice(&[b; 32]);
        }
        data
    }

    #[test]
    fn reads_both_authorities_when_present() {
        let (mint, freeze) = mint_authorities(&mint_bytes(Some(7), Some(9))).expect("82 bytes");
        assert_eq!(mint, Some(Address::new([7; 32])));
        assert_eq!(freeze, Some(Address::new([9; 32])));
    }

    #[test]
    fn reads_both_as_absent_when_revoked() {
        let (mint, freeze) = mint_authorities(&mint_bytes(None, None)).expect("82 bytes");
        assert_eq!(mint, None);
        assert_eq!(freeze, None);
    }

    #[test]
    fn refuses_an_account_shorter_than_a_mint() {
        let short = &[0u8; 10];
        assert!(mint_authorities(short).is_err());
    }
}
