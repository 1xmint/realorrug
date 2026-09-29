// SPDX-License-Identifier: Apache-2.0
//! Talking to the forecasting game's routes (design 0032 §11, §13).
//!
//! # Why this is not `api.ts`'s `get()`
//!
//! `api.ts` degrades every failure to an honest empty shape, and its base is
//! `""` when unset, which means "same origin". Neither is right here:
//!
//! - **No base means the game is off.** The game needs the API on a subdomain
//!   of the site's own registrable domain (the session cookie is `SameSite=Lax`,
//!   §11), so a same-origin default would call a server that is not there and
//!   could only ever be answered by whatever static host serves the page.
//!   AGENTS.md §3 rule 7: deny by default when config is missing. [`gameBase`]
//!   returns `null`, and every caller renders "the game is not running" and
//!   nothing else. There is no mock data anywhere in this file.
//! - **A refusal carries the sentence the player needs.** A 409 ("the first
//!   call stands"), a 403 (the window closed) and a 503 are different facts and
//!   `get()` would flatten them into one `null`.
//!
//! The base is read when a call is made, not at module load, so a test can set
//! it and a build that leaves it out is off for every call.

/** How long to wait for the game server. Longer than `api.ts`: a sign-in read is metered and slow. */
const TIMEOUT_MS = 8000;

/** The header the server checks on every POST (design 0032 §9, §11). */
export const CSRF_HEADER = "x-csrf-token";

/**
 * Where the game's API lives, or `null` when the game is off.
 *
 * `https` only, with `http` allowed for a loopback host so a local
 * `realorrug-serve` can be tried. A malformed value is off, not repaired: the
 * session cookie is sent to this host, so a guessed host is a leak.
 */
export function gameBase(): string | null {
  const configured: unknown = import.meta.env["VITE_API_BASE"];
  if (typeof configured !== "string" || configured.trim() === "") return null;
  let url: URL;
  try {
    url = new URL(configured.trim());
  } catch {
    return null;
  }
  const loopback =
    url.hostname === "localhost" ||
    url.hostname === "127.0.0.1" ||
    url.hostname === "[::1]";
  if (url.protocol !== "https:" && !(url.protocol === "http:" && loopback)) {
    return null;
  }
  return url.origin;
}

/** An address on the game's API, or `null` when the game is off. */
export function gameHref(path: string): string | null {
  const base = gameBase();
  return base === null ? null : `${base}${path}`;
}

/**
 * How a call ended. Four different facts, so a page cannot draw an
 * unreachable server as an empty round.
 */
export type Reply<T> =
  | { readonly kind: "off" }
  | { readonly kind: "unreachable" }
  | { readonly kind: "refused"; readonly status: number; readonly error: string | null }
  | { readonly kind: "ok"; readonly status: number; readonly body: T };

interface Options {
  readonly method?: "GET" | "POST";
  /** Send the session cookie. Session routes only; public reads carry none. */
  readonly session?: boolean;
  /** The CSRF token from `/auth/me`, for a POST. */
  readonly csrf?: string;
  readonly body?: unknown;
}

/** One call. Never throws. */
export async function call<T>(path: string, options: Options = {}): Promise<Reply<T>> {
  const base = gameBase();
  if (base === null) return { kind: "off" };
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), TIMEOUT_MS);
  try {
    const headers: Record<string, string> = {};
    if (options.csrf !== undefined) headers[CSRF_HEADER] = options.csrf;
    if (options.body !== undefined) headers["content-type"] = "application/json";
    const init: RequestInit = {
      method: options.method ?? "GET",
      // `include`, not the default `same-origin`: the API is a different
      // origin from the page (same site, not same origin), and without this
      // the browser sends no cookie at all.
      credentials: options.session === true ? "include" : "omit",
      headers,
      signal: controller.signal,
    };
    if (options.body !== undefined) init.body = JSON.stringify(options.body);
    const response = await fetch(`${base}${path}`, init);
    let parsed: unknown = null;
    try {
      parsed = await response.json();
    } catch {
      parsed = null;
    }
    if (!response.ok) {
      const error =
        typeof parsed === "object" && parsed !== null && "error" in parsed
          ? (parsed as { error: unknown }).error
          : null;
      return {
        kind: "refused",
        status: response.status,
        error: typeof error === "string" ? error : null,
      };
    }
    if (parsed === null) return { kind: "unreachable" };
    return { kind: "ok", status: response.status, body: parsed as T };
  } catch {
    return { kind: "unreachable" };
  } finally {
    clearTimeout(timer);
  }
}

/** A round id the server could hold. Anything else never costs a request. */
export function roundIdShaped(id: string): boolean {
  return /^[A-Za-z0-9._-]{1,64}$/.test(id);
}

/** The server's unix seconds as an ISO string, for `measuredAgo`. `null` if it is not a number. */
export function isoOf(seconds: unknown): string | null {
  if (typeof seconds !== "number" || !Number.isFinite(seconds)) return null;
  return new Date(seconds * 1000).toISOString();
}

/** The two sides the server accepts (`parse_side` in forecast.rs). */
export type Side = "real" | "rug";

/** A coin in a round. */
export interface Coin {
  readonly chain: string;
  readonly token: string;
}

/** `GET /v1/rounds/{round}`. Never carries a count. */
export interface RoundInfo {
  readonly round: string;
  readonly window_close: number;
  readonly closed: boolean;
  readonly coins: readonly Coin[];
  readonly read_at: number;
  readonly newest_at: number | null;
}

/** `GET /v1/rounds/{round}/forecasts`, once the window has closed. */
export interface RoundForecasts {
  readonly round: string;
  readonly closed: boolean;
  readonly forecasts?: readonly (Coin & { readonly side: Side })[];
  readonly read_at: number;
  readonly newest_at: number | null;
}

/** `GET /v1/rounds/{round}/outcomes`, once the window has closed. */
export interface RoundOutcomes {
  readonly round: string;
  readonly closed: boolean;
  readonly outcomes?: readonly (Coin & {
    readonly reading: string;
    readonly settled_at: number;
  })[];
  readonly read_at: number;
  readonly newest_at: number | null;
}

/** One line of `GET /v1/board`. */
export interface BoardLine {
  readonly board_id: string;
  readonly hits: number;
  readonly misses: number;
  readonly n: number;
}

/** `GET /v1/board`. */
export interface Board {
  readonly board: readonly BoardLine[];
  readonly read_at: number;
  readonly newest_at: number | null;
}

/** One of the caller's own calls. */
export interface MyCall extends Coin {
  readonly side: Side;
  readonly submitted_at: number;
  readonly window_close: number;
}

/** `GET /forecast/mine?round=`. */
export interface Mine {
  readonly round: string;
  readonly board_id: string;
  readonly forecasts: readonly MyCall[];
  readonly read_at: number;
}

/** `GET /auth/me` when signed in. */
export interface Me {
  readonly signed_in: true;
  readonly handle: string;
  readonly csrf_token: string;
}

/** `GET /v1/privacy`, as returned. */
export interface PrivacyNotice {
  readonly kept?: readonly { readonly field: string; readonly why: string; readonly ends: string }[];
  readonly not_kept?: readonly string[];
  readonly sessions?: string;
  readonly deletion?: string;
  readonly public?: string;
}

export const rounds = {
  info: (id: string) => call<RoundInfo>(`/v1/rounds/${encodeURIComponent(id)}`),
  forecasts: (id: string) =>
    call<RoundForecasts>(`/v1/rounds/${encodeURIComponent(id)}/forecasts`),
  outcomes: (id: string) =>
    call<RoundOutcomes>(`/v1/rounds/${encodeURIComponent(id)}/outcomes`),
};

export const board = () => call<Board>("/v1/board");

export const privacy = () => call<PrivacyNotice>("/v1/privacy");

export const me = () => call<Me>("/auth/me", { session: true });

export const mine = (round: string) =>
  call<Mine>(`/forecast/mine?round=${encodeURIComponent(round)}`, { session: true });

export const signOut = (csrf: string) =>
  call<{ signed_out: boolean }>("/auth/logout", { method: "POST", session: true, csrf, body: {} });

/** Files one call. The body is `{round, chain, token, side}` and nothing else (deny_unknown_fields). */
export const fileCall = (
  csrf: string,
  round: string,
  coin: Coin,
  side: Side,
) =>
  call<{ saved: boolean }>("/forecast", {
    method: "POST",
    session: true,
    csrf,
    body: { round, chain: coin.chain, token: coin.token, side },
  });
