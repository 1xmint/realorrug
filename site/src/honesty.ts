// SPDX-License-Identifier: Apache-2.0
//! The functions that decide what this site *claims*.
//!
//! Separated from the components that render them, for the reason `web`'s file
//! of the same name gives: a snapshot of a `<div>` fails when somebody renames a
//! class and passes when the page lies. Each function here has a wrong version
//! that looks right, which is the only reason to pull one out.
//!
//! This is a public marketing surface, which makes it the *most* important place
//! in the repository to get this right rather than the least. Every figure it
//! shows is a claim made to a stranger about somebody else's project.

/** A count with thousands separators, in the reader's own locale. */
export function count(value: number): string {
  return value.toLocaleString();
}

/**
 * How long ago a measurement was taken, in words.
 *
 * Every figure on this site is printed with one of these beside it. A number
 * with no date is the failure `0024` records in capitals: the note that measured
 * these quantities before it was wrong by 2.7× nine days later, and a reader who
 * cannot see the date cannot know to doubt it.
 *
 * Returns `null` for an unparseable or future timestamp rather than guessing.
 * A clock skew rendering "in 3 hours" would look like a bug in the data, which
 * is worse than saying nothing.
 */
export function measuredAgo(
  iso: string,
  now: Date = new Date(),
): string | null {
  const then = Date.parse(iso);
  if (Number.isNaN(then)) return null;
  const seconds = Math.floor((now.getTime() - then) / 1000);
  if (seconds < 0) return null;
  if (seconds < 90) return "just now";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 90) return `${minutes} minutes ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 36) return `${hours} hours ago`;
  const days = Math.floor(hours / 24);
  return `${days} days ago`;
}

/* ------------------------------------------------------------------------- *
 * Links.
 *
 * Every function below returns `string | null`, and `null` means "do not
 * render a link". That shape is the whole point: the alternative is a
 * component interpolating an API field straight into an `href`, and the
 * fields these are built from arrive from `/v1/public/*` — a document this
 * site does not write. A `javascript:` URL in a field the site trusts is one
 * stored value away from running in a reader's browser.
 *
 * `wouter` and React escape *text*, and neither escapes a URL scheme. React
 * warns on `javascript:` in newer versions and does not block it, and a warning
 * in somebody else's console is not a defence.
 *
 * These are also the reason the site never builds an `href` by template in a
 * component. If a link is not made here, it is not made.
 * ------------------------------------------------------------------------- */

/** The base58 alphabet, which excludes the confusable characters `0OIl`. */
const BASE58 = /^[1-9A-HJ-NP-Za-km-z]+$/;

/**
 * A URL that is safe to put in an `href`, or `null`.
 *
 * `https` only, and the host must be one the caller named. Parsing with `URL`
 * rather than matching a prefix is deliberate: `https://evil.example/#@x.com`
 * passes a `startsWith` check and is not x.com, and every hand-rolled version
 * of this function in the wild is a prefix check.
 */
export function safeHref(url: string, hosts: readonly string[]): string | null {
  let parsed: URL;
  try {
    parsed = new URL(url);
  } catch {
    return null;
  }
  if (parsed.protocol !== "https:") return null;
  if (!hosts.includes(parsed.hostname)) return null;
  return parsed.toString();
}

/**
 * The repository this site is built from.
 *
 * A constant rather than a literal in a component, because of the rule directly
 * above: if a link is not made in this file, it is not made. These three are
 * the only fixed destinations the site knows — every other URL is derived from
 * a document `/v1/public/*` served, and derived by a function that can refuse.
 *
 * They are also the whole of the operator's published contact surface. There is
 * **no email address anywhere in this repository**, which is why `/contact`
 * says so rather than inventing one.
 */
export const SOURCE = "https://github.com/hey-vera/radar";

/** Where to report something wrong with the site or a number on it. */
export const ISSUES = `${SOURCE}/issues`;

/** Where to report a vulnerability privately. `SECURITY.md` names this form. */
export const ADVISORY = `${SOURCE}/security/advisories/new`;

/**
 * A link to an X account from its handle, or `null`.
 *
 * X's own rule: 1–15 characters, letters, digits and underscore. A 16th
 * character is not a handle, and rendering it as one produces a link to a
 * profile that does not exist — on the page that is supposed to be the account's
 * introduction.
 */
export function handleHref(handle: string): string | null {
  if (!/^[A-Za-z0-9_]{1,15}$/.test(handle)) return null;
  return `https://x.com/${handle}`;
}

/**
 * A link to an X account from its numeric id, or `null`.
 *
 * The leaderboard has ids and may not have handles: `close()` records the
 * summoner's id because that is what a mention carries, and the handle is a
 * second field that can be missing. The `i/user` form resolves without a
 * handle, exactly as `public.rs` uses `i/web/status` for a post.
 *
 * Digits only. An id is a number, and anything else in that position came from
 * somewhere it should not have.
 */
export function userHref(id: string): string | null {
  if (!/^[0-9]{1,25}$/.test(id)) return null;
  return `https://x.com/i/user/${id}`;
}

/**
 * A link to a transaction on Robinhood Chain's explorer, or `null`.
 *
 * The token, the bot's wallet and the weekly payout are all on Robinhood
 * Chain now (ADR 0029, ADR 0025), so a payout row links Blockscout rather
 * than Solscan. `robinhoodchain.blockscout.com` is the explorer research
 * 0035 §6 names; a hash is 32 bytes of hex, exactly 66 characters with the
 * `0x` prefix, and the bound is exact for the same reason `evmShaped` below
 * is: a run that is too long is not a hash, and truncating it to a plausible
 * length hands a reader a link to somebody else's transaction.
 */
export function explorerTx(hash: string): string | null {
  if (!/^0x[0-9a-fA-F]{64}$/.test(hash)) return null;
  return `https://robinhoodchain.blockscout.com/tx/${hash}`;
}

/** A link to an account on Robinhood Chain's explorer, or `null`. */
export function explorerAccount(address: string): string | null {
  if (!evmShaped(address)) return null;
  return `https://robinhoodchain.blockscout.com/address/${address}`;
}

/**
 * A link to a transaction on Solscan, or `null`.
 *
 * Kept for the general checker (`Check.tsx`, `ui/index.tsx`), which still
 * reads either chain (`CheckResult.chain`) — a stranger can paste a Solana
 * mint and get an answer about it. This project's own token, wallet and
 * payouts are Robinhood Chain now; see [`explorerTx`] and
 * [`explorerAccount`] for those.
 *
 * Signatures are 64 bytes in base58, which is 87 or 88 characters. The bound is
 * exact rather than "long enough", for the reason `mention.rs` gives about
 * addresses: a run that is too long is not a signature, and truncating it to a
 * plausible length hands a reader a link to somebody else's transaction.
 */
export function solscanTx(signature: string): string | null {
  if (signature.length < 86 || signature.length > 88) return null;
  if (!BASE58.test(signature)) return null;
  return `https://solscan.io/tx/${signature}`;
}

/** A link to an account on Solscan, or `null`. Same rule as a mint. */
export function solscanAccount(address: string): string | null {
  if (!mintShaped(address)) return null;
  return `https://solscan.io/account/${address}`;
}

/**
 * Whether this is shaped like a Solana address.
 *
 * The same rule `mention.rs` applies to a summons, restated here because the
 * summon box has to decide *before* anything is sent whether the bot would read
 * what the reader pasted. 32 to 44 base58 characters, bounds exact.
 *
 * This is a shape check and not an existence check, and the interface must say
 * so: a well-formed address for a coin that does not exist is not caught here,
 * and the bot answers that case by refusing rather than by inventing a record.
 */
export function mintShaped(text: string): boolean {
  const t = text.trim();
  return t.length >= 32 && t.length <= 44 && BASE58.test(t);
}

/**
 * A prefilled X post that summons the account about a token, or `null`.
 *
 * `null` when the handle is not configured or the text is not address-shaped —
 * a summon button that posts `@undefined` would be worse than no button. The
 * handle is a parameter rather than a constant here because this site does not
 * know it: see [`account`].
 *
 * **Either chain.** This gated on `mintShaped` alone until 2026-09-17, so a
 * Robinhood Chain `0x` address — the chain the bot mainly answers about — got
 * no button at all from the front page's own box, while `Check.tsx` next door
 * accepted it. The bot dispatches on the shape of the address it is handed
 * (one list, one path, both chains), and this button only writes the post the
 * reader sends, so the shapes it accepts are the shapes the bot reads.
 */
export function summonIntent(handle: string, mint: string): string | null {
  if (handleHref(handle) === null) return null;
  if (!mintShaped(mint) && !evmShaped(mint)) return null;
  const text = encodeURIComponent(`@${handle} ${mint.trim()}`);
  return `https://x.com/intent/post?text=${text}`;
}

/**
 * The account's handle, from the build environment, or `null`.
 *
 * **The handle is `realorrug`** — renamed from `thecabalhunter`, confirmed by the
 * operator on 2026-09-13.
 * It is deliberately *not* hard-coded here anyway, and that is the point of
 * this function rather than an omission.
 *
 * Nothing on the Rust side needs a handle: the analyst identifies the account
 * by `REALORRUG_X_USER_ID`, and `realorrug-serve` builds reply links as
 * `x.com/i/web/status/<id>` precisely so it never has to know one. This site is
 * the only surface that wants a name, and a name is the one thing about the
 * account that can change without anything breaking loudly. Hard-coding it
 * would mean a rename becomes a code change and a deploy, and in the meantime
 * every link on the page points at whoever took the old handle — the one link
 * here that cannot be walked back.
 *
 * So it is build configuration, validated on the way in against X's own rule,
 * and everything that needs it renders an honest alternative when it is
 * absent. AGENTS.md rule 8: deny by default when config is missing. Set
 * `VITE_X_HANDLE` in the Cloudflare Pages environment; it is inlined at build
 * time, so it takes a redeploy rather than a restart.
 */
export function account(): string | null {
  const configured = import.meta.env["VITE_X_HANDLE"];
  if (typeof configured !== "string") return null;
  const handle = configured.trim().replace(/^@/, "");
  return handleHref(handle) === null ? null : handle;
}

/**
 * Whether text is shaped like a Robinhood Chain contract address.
 *
 * Shape only: forty hex digits after `0x`. Whether anything lives there is the
 * server's question (design 0023 §2), and a shape check here only saves a
 * stranger a round trip for something that was never an address.
 */
export function evmShaped(text: string): boolean {
  return /^0x[0-9a-fA-F]{40}$/.test(text.trim());
}

/**
 * Wei as ETH, at the precision a prize is worth quoting to.
 *
 * `api.ts`'s `sol()` divided by `1_000_000_000` for Solana's lamports; wei is
 * `10^18`, not `10^9`, so this is a new function rather than a relabelled
 * one — dividing by the old constant would understate every figure by nine
 * orders of magnitude.
 */
export function eth(wei: number): string {
  return (wei / 1_000_000_000_000_000_000).toFixed(4);
}

/**
 * Wording ADR 0038 retires along with the weekly prize.
 *
 * The prize, the pool and every holder benefit are gone; if any of these
 * words shows up on a page again it is either a leftover sentence from before
 * the retirement or a new claim nobody decided to make, and either way the
 * page is wrong before a human reads it. Checked case-insensitively, as a
 * substring, against rendered page text — see `empty.test.tsx`'s
 * "no page delivers a verdict" checks, which run this list against every
 * page rather than trusting each page's author to remember it by hand.
 */
export const FORBIDDEN_CLAIMS: readonly string[] = [
  "prize",
  "payout",
  "buyback",
  "holders earn",
  "yield",
];
