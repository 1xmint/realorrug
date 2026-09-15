<!-- SPDX-License-Identifier: Apache-2.0 -->
# Deploying realorrug

Three processes, each its own unit. **Installed on the box since 2026-09-14:
`realorrug-serve`, as a user unit, and `realorrug-analyst`** (below). The payout
is not installed; this is the runbook for when it is.

## What runs today

`realorrug-serve` runs as guardian's systemd **user** unit,
[`deploy/user/realorrug-serve.service`](user/realorrug-serve.service), on
`127.0.0.1:8090`, beside `radar-serve` on `8402`. A user unit because the
system unit needs sudo with a password and the box's passwordless sudo belongs
to other services; guardian lingers, so the unit starts at boot and is
supervised. Binary at `~/realorrug/bin/realorrug-serve`, with the release's
`BUILD-INFO.txt` beside it.

Installed from `release-linux` run 34793325535, commit `ae448f0`; `/health`
reported that build, and the five `/v1/public/*` documents were byte-identical
to `radar-serve`'s on install.

**The live site reaches it** since 2026-09-14. The site calls
`https://radar.heyvera.org`; the root-owned tunnel config
`/etc/cloudflared/config.yml` holds a rule, above that hostname's catch-all,
sending `/v1/public/*` to `http://localhost:8090`. Everything else on the
hostname still goes to `radar-serve`. On the switch, the five documents fetched
through the hostname matched `8090`'s own answers apart from their
`measured_at` time, and the tunnel held a connection to `8090` and none to
`8402`.

```bash
# Which rule a URL matches (on the box)
sudo cloudflared tunnel --config /etc/cloudflared/config.yml ingress rule https://radar.heyvera.org/v1/public/stats
# Roll back to radar-serve: the config before the rule was kept beside it
sudo cp /etc/cloudflared/config.yml.bak-realorrug /etc/cloudflared/config.yml && sudo systemctl restart cloudflared
```

```bash
# Check
ssh guardian-vps-tail 'systemctl --user is-active realorrug-serve; curl -s localhost:8090/health'
# Upgrade: download the artifact, then
scp realorrug-serve guardian-vps-tail:/tmp/ && ssh guardian-vps-tail \
  'install -m 0755 /tmp/realorrug-serve ~/realorrug/bin/ && systemctl --user restart realorrug-serve'
# Remove
ssh guardian-vps-tail 'systemctl --user disable --now realorrug-serve'
```

**The analyst runs** since 2026-09-14 15:32 UTC, as the system unit
[`realorrug-analyst.service`](realorrug-analyst.service), binary
`~/bin/realorrug-analyst` from release run 34852198731 (`1706340`). It
replaced Radar's `/etc/systemd/system/radar-analyst.service`, which is
stopped and disabled; the old stopped at 15:32:13 and the new started at
15:32:14. Both started `LIVE` with two operator ids, and the new one moved the
mention cursor two minutes later with nothing in its journal but its startup
lines. Both units read `/etc/radar/analyst.env` and write the same
directories, so they must never run together. The binary has no `--help`:
any invocation starts the daemon.

```bash
# Roll back to Radar's analyst (on the box)
sudo systemctl disable --now realorrug-analyst && sudo systemctl enable --now radar-analyst
```

| unit | binary | what it does | writes |
|---|---|---|---|
| `realorrug-analyst.service` | `realorrug-analyst` | answers summoned mentions on X with measured facts | `data/analyst`, `data/contest` |
| `realorrug-payout.service` + `realorrug-payout.timer` | `realorrug-payout --due` | claims a claimed, unpaid week's fees from the escrow and pays them, signed through Turnkey | `data/contest` |
| `realorrug-serve.service` | `realorrug-serve` | the public site's five documents | nothing |

## What it reads from Radar, and how

The bot does not import Radar. It reads **two files Radar publishes**, at paths
relative to its working directory:

- `docs/research/data/creator-index.json` and `population.json`, written every
  six hours by Radar's `theradar:deploy/radar-creator-index.timer`.
- `docs/research/data/0024-base-rates.json`, a dated snapshot. A copy is
  committed here, so the bot runs without Radar; the box's copy wins when the
  working directory is Radar's.

That is why the units keep `WorkingDirectory=/home/guardian/radar`. A file
format is the whole contract: if Radar stops publishing, the bot's replies say
nothing about who launched a token, and say so, rather than failing.

Radar's `radar seven-days-later` timer still joins this bot's reply log with
Radar's store and writes the file the daily post reads.

## Install

```bash
# Built by CI on a push to main; download the artifact, then:
sudo install -m 0755 realorrug-analyst realorrug-payout realorrug-serve /usr/local/bin/
sudo install -m 0644 deploy/realorrug-analyst.service deploy/realorrug-payout.service deploy/realorrug-payout.timer deploy/realorrug-serve.service /etc/systemd/system/
sudo install -m 0640 -o root -g guardian deploy/analyst.env.example /etc/radar/analyst.env
sudo systemctl daemon-reload
sudo systemctl enable --now realorrug-serve realorrug-analyst
```

The payout runs as its own user, `realorrug-payout`, which owns nothing but its
Turnkey API key and the contest directory. **Do not enable the timer until
launch**: the token exists, the gas float is funded, and the setup proof below
has passed.

### The payout's key is in Turnkey

[ADR 0025](../docs/adr/0025-the-robinhood-payout-signs-through-turnkey.md). The
wallet key never touches the box. What the box holds is a Turnkey API key that
can ask Turnkey to sign two kinds of transaction and nothing else.

Set up in Turnkey's dashboard, by the operator, on a passkey:

1. An organisation, with the operator as root user.
2. One wallet with one Ethereum account. Its address is `RADAR_PAYOUT_ADDRESS`,
   and the token's creator fee recipient.
3. A user `realorrug-payout`, not in the root quorum, holding one API key on
   the **secp256k1** curve. Turnkey's CLI defaults to P-256, which the payout
   cannot use, so name the curve. Generate it where it will live, so the private
   half never crosses a network, and paste the printed `publicKey` into the
   user's API key in the dashboard:

   ```bash
   turnkey generate api-key --organization <org id> --key-name realorrug-payout --curve secp256k1
   sudo install -m 0400 -o realorrug-payout ~/.config/turnkey/keys/realorrug-payout.private /etc/radar/turnkey.key
   ```

   The file goes in as the CLI wrote it (64 hex digits, `:secp256k1`); a `0x`
   prefix or no suffix also loads. The process refuses a key marked as another
   curve, a file group or others can read, and a key whose public half is not
   `TURNKEY_API_PUBLIC_KEY`. Delete the CLI's copy once installed. **A key made
   on your own root user is not this key**: the root quorum is not bound by the
   policy, so it could sign anything.
4. One ALLOW policy for that user, and no other policy naming it. Check the
   expression in Turnkey's policy editor while writing it:

   ```text
   consensus: approvers.any(user, user.id == '<realorrug-payout user id>')
   condition: activity.type == 'ACTIVITY_TYPE_SIGN_TRANSACTION_V2'
     && eth.tx.chain_id == 4663
     && ((eth.tx.to == '0xd3afeb2a57f70ef218aa82451c51b2fb0416ac9e'
          && eth.tx.value == 0
          && eth.tx.function_signature == '0x379607f5')
         || eth.tx.data == '')
   ```

   That is `claim(uint256)` on the Pons fee escrow, or a plain ETH transfer, on
   Robinhood Chain. No token approvals, no other contracts, no other chains.
   There is deliberately no value cap; ADR 0025 says why.
5. Wallet and key export stay denied to everyone but root.

Then the setup proof. It needs only the Turnkey variables and
`RADAR_PAYOUT_ADDRESS`, sends nothing to any chain, and costs nothing:

```bash
sudo systemd-run --pty --wait --uid=realorrug-payout -p EnvironmentFile=/etc/radar/payout.env -E TURNKEY_API_KEY=/etc/radar/turnkey.key /usr/local/bin/realorrug-payout --setup-proof
```

It passes only when `whoami` answers, a call to the Pons factory is **denied**,
and `claim(0)` at nonce 1,000,000 is **allowed**, returned as the transaction
asked for and signed by the wallet. Record the three lines in
[research 0037](../docs/research/0037-a-payouts-gas-read-from-mainnet.md) §4,
without the organisation id or any key. The same document sizes the gas float:
0.001 ETH covers about 95 weeks at the September 2026 base fee.

### A payout that stopped part way

A run writes `data/contest/<week>.pending.json` before each transaction it
sends, and the next run finishes that week before anything else. It never claims
twice. Two cases stop for the operator:

- **`nonce N was used by a transaction other than ...`**: something else was sent
  from the wallet. Look the wallet up on the explorer; if the pending claim is
  truly dead, delete the pending file.
- **`locked`**: `data/contest/payout.lock` exists. If no payout is running, a run
  died holding it; check the wallet and any pending file, then delete the lock.

A claim or transfer made by hand is recorded with
`realorrug contest record-payout --week N --wallet <address> --rpc <url> --claim-tx <hash> --transfer-tx <hash>`,
which reads both back through the same checks.

The environment variables keep their `RADAR_` prefix, so an existing
`/etc/radar/analyst.env` works unchanged.
