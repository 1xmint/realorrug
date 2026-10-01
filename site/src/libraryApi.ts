// SPDX-License-Identifier: Apache-2.0
/** Explicitly public dossiers; failed reads never become empty success. */
export const API = import.meta.env["VITE_API_BASE"] ?? "";
export type Network = "solana" | "base" | "ethereum" | "robinhood";
export interface CaseKey { chain: Network; address: string }
export interface Observation { id: string; kind: string; source: string; at: number; read_point: string | null; value: unknown; gap: string | null; version: string }
export interface Finding { kind: string; text: string; evidence: string[]; counterevidence: string[]; status: string }
export interface Assessment { request_id: string; level: string; reply: string; observations: Observation[]; findings: Finding[]; reused: string[]; decisions: string[]; rpc_calls: number; elapsed_ms: number; complete: boolean }
export interface Dossier { case: CaseKey; revision: number; updated_at: number; assessment: Assessment | null }
export interface CaseSummary { case: CaseKey; revision: number; updated_at: number; assessment: Pick<Assessment,"level"|"complete"> | null }
export interface CaseEvent { revision: number; kind: string; at: number; payload: unknown; hash: string; previous_hash: string }
export interface Job { request: { id: string; case: CaseKey; question: string }; status: string; admitted_at: number; updated_at: number; model_attempts: number; error: string | null }
export interface Session { signed_in: boolean; handle: string; csrf_token: string }

export async function read<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`${API}${path}`, { ...init, credentials: "include", signal: AbortSignal.timeout(8000) });
  const value: unknown = await response.json().catch(() => {
    throw new Error("The Library service is unavailable or returned an unreadable response. Please try again.");
  });
  if (!response.ok) {
    const message = typeof value === "object" && value !== null && "error" in value && typeof value.error === "string" ? value.error : `Request failed (${response.status})`;
    throw new Error(message);
  }
  return value as T;
}
export const casePath = (key: CaseKey) => `/library/${key.chain}/${encodeURIComponent(key.address)}`;
export const apiCasePath = (key: CaseKey) => `/v1/cases/${key.chain}/${encodeURIComponent(key.address)}`;
export function publicSource(url: unknown): string | null {
  if (typeof url !== "string") return null;
  try { const parsed = new URL(url); return parsed.protocol === "https:" && ["x.com", "sourcify.dev", "repo.sourcify.dev"].includes(parsed.hostname) ? parsed.href : null; } catch { return null; }
}
