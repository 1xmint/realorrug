#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
#
# Makes the payout's Turnkey API key on the box, where it will live, so the
# private half never crosses a network or a clipboard. From a checkout (sudo
# asks for a password, so ssh needs -t):
#
#   scp deploy/make-payout-key.sh guardian-vps-tail:
#   ssh -t guardian-vps-tail 'bash make-payout-key.sh; rm make-payout-key.sh'
#
# It creates the `realorrug-payout` system user if missing, writes the private
# key to /etc/realorrug/turnkey.key (0400, that user's), and prints the public
# key to paste into the user's API key in Turnkey's dashboard. It never prints
# the private key, and refuses to overwrite one that exists: a replaced key
# would silently orphan the one Turnkey knows.
#
# P-256 because Turnkey's dashboard files every pasted public key as P-256
# (ADR 0025). OpenSSL instead of Turnkey's CLI because the box already has
# OpenSSL, and the CLI would be one more download holding the key's secret half.
# The file is the CLI's format (64 hex digits, `:p256`), which the payout loads
# as written.
set -euo pipefail

dir=/etc/realorrug
key=$dir/turnkey.key

if sudo test -e "$key"; then
    # This script's first version made secp256k1 keys. The dashboard filed that
    # key as P-256, so no request it stamps is accepted: it is removed, not
    # kept. Any other key is refused.
    if sudo grep -q ':secp256k1$' "$key"; then
        sudo shred -u "$key"
        echo "removed the earlier secp256k1 key, which Turnkey cannot use"
    else
        echo "refusing: $key already exists; revoke its API key in Turnkey before replacing it" >&2
        exit 1
    fi
fi
id realorrug-payout >/dev/null 2>&1 ||
    sudo useradd --system --no-create-home --shell /usr/sbin/nologin realorrug-payout
sudo install -d -m 0755 -o root -g root "$dir"

umask 077
tmp=$(mktemp -d)
trap 'shred -u "$tmp"/* 2>/dev/null || rm -f "$tmp"/*; rmdir "$tmp"' EXIT

hex() { od -An -tx1 -v | tr -d ' \n'; }

openssl ecparam -name prime256v1 -genkey -noout -out "$tmp/k.pem"
openssl ec -in "$tmp/k.pem" -outform DER -out "$tmp/k.der" 2>/dev/null

# A P-256 key in SEC1 DER is always 30 77 02 01 01 04 20, then the 32-byte
# scalar. Checked rather than assumed, because the offset is the whole parse.
header=$(head -c 7 "$tmp/k.der" | hex)
if [ "$header" != 30770201010420 ]; then
    echo "unexpected key encoding ($header); nothing installed" >&2
    exit 1
fi
scalar=$(tail -c +8 "$tmp/k.der" | head -c 32 | hex)
printf '%s:p256\n' "$scalar" >"$tmp/key"
unset scalar

public=$(openssl ec -in "$tmp/k.pem" -pubout -conv_form compressed -outform DER 2>/dev/null | tail -c 33 | hex)

sudo install -m 0400 -o realorrug-payout -g realorrug-payout "$tmp/key" "$key"

shaped=$(sudo grep -c '^[0-9a-f]\{64\}:p256$' "$key" || true)
if [ "$shaped" != 1 ]; then
    echo "the installed key is not 64 hex digits and :p256; delete $key and rerun" >&2
    exit 1
fi
sudo ls -l "$key"
echo
echo "publicKey (paste into Turnkey, and set TURNKEY_API_PUBLIC_KEY to it):"
echo "$public"
