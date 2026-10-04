// SPDX-License-Identifier: Apache-2.0
import { useEffect, useRef, useState, type FormEvent } from "react";
import { Link, useRoute } from "wouter";
import { API, apiCasePath, casePath, read, publicSource, type Assessment, type CaseKey, type CaseSummary, type Dossier, type CaseEvent, type Job, type Network, type Session } from "./libraryApi";
import { useTitle } from "./title";

const field = "w-full rounded border border-[var(--color-line)] bg-[var(--color-raised)] px-3 py-2 text-sm";
const button = "rounded border border-[var(--color-line)] px-4 py-2 text-sm disabled:opacity-40";
const time = (seconds: number) => new Date(seconds * 1000).toLocaleString();
const errorText = (error: unknown) => error instanceof Error ? error.message : "The request could not be completed.";
const networks: Network[] = ["solana", "base", "ethereum", "robinhood"];

export function Library() {
  useTitle("Public investigation Library");
  const [chain, setChain] = useState("");
  const [query, setQuery] = useState("");
  const [search, setSearch] = useState({ chain: "", query: "" });
  const [cases, setCases] = useState<CaseSummary[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [next, setNext] = useState<number | null>(null);
  const generation=useRef(0);const [loadingMore,setLoadingMore]=useState(false);
  useEffect(() => {
    let active = true; generation.current+=1;setCases(null); setError(null);setNext(null);setLoadingMore(false);
    const params = new URLSearchParams({ q: search.query, ...(search.chain ? { chain: search.chain } : {}) });
    void read<{ cases: CaseSummary[]; next_before: number | null }>(`/v1/library?${params}`).then(result => {
      if (active) { setCases(result.cases); setNext(result.next_before); }
    }).catch(error => { if (active) setError(errorText(error)); });
    return () => { active = false; };
  }, [search]);
  async function more() {
    if (next === null || loadingMore) return;const current=generation.current;setLoadingMore(true);
    try {
      const params = new URLSearchParams({ q: search.query, before: String(next), ...(search.chain ? { chain: search.chain } : {}) });
      const result = await read<{ cases: CaseSummary[]; next_before: number | null }>(`/v1/library?${params}`);
      if(current===generation.current){setCases(previous => [...(previous ?? []), ...result.cases]); setNext(result.next_before);}
    } catch (error) { if(current===generation.current)setError(errorText(error)); }finally{if(current===generation.current)setLoadingMore(false);}
  }
  return <section className="mx-auto w-full max-w-5xl space-y-8 px-6 py-12 [overflow-wrap:anywhere]">
    <header><p className="text-sm text-[var(--color-dim)]">Free public research</p><h1 className="display text-4xl">Investigation Library</h1>
      <p className="mt-4 max-w-2xl">Browse evolving token dossiers, measured observations, community leads, corrections and unanswered questions. A partial investigation is not a safety certificate.</p></header>
    <form onSubmit={event => { event.preventDefault(); setSearch({ chain, query }); }} className="flex flex-wrap gap-3">
      <label className="min-w-[14rem] flex-1">Token address<input className={field} value={query} maxLength={128} onChange={event => setQuery(event.target.value)} /></label>
      <label>Network<select className={field} value={chain} onChange={event => setChain(event.target.value)}><option value="">All networks</option>{networks.map(network => <option key={network}>{network}</option>)}</select></label>
      <button className={button}>Search</button>
    </form>
    {error && <p role="alert">Library unavailable: {error}</p>}
    {!error && cases === null && <p role="status">Loading public cases…</p>}
    {cases?.length === 0 && <p>No recorded cases match this search.</p>}
    <div className="space-y-3">{cases?.map(dossier => <article key={`${dossier.case.chain}:${dossier.case.address}`} className="rounded border border-[var(--color-line)] p-4">
      <Link href={casePath(dossier.case)} className="font-mono text-sm break-all underline">{dossier.case.chain}: {dossier.case.address}</Link>
      <p className="mt-2 text-sm">{dossier.assessment ? `${dossier.assessment.complete ? "Scope covered" : "Partial coverage"} · ${dossier.assessment.level}` : "Requested; no assessment yet"}</p>
      <p className="text-xs text-[var(--color-dim)]">Updated {time(dossier.updated_at)} · revision {dossier.revision}</p>
    </article>)}</div>
    {next !== null && <button disabled={loadingMore} className={button} onClick={() => void more()}>Older cases</button>}
    <RequestForm />
    <Methodology />
  </section>;
}

export function TokenDossier() {
  const [, params] = useRoute("/library/:chain/:address");
  const key: CaseKey = { chain: (params?.chain ?? "solana") as Network, address: params?.address ?? "" };
  const [dossier, setDossier] = useState<Dossier | null>(null);
  const [events, setEvents] = useState<CaseEvent[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [next, setNext] = useState<number | null>(null);
  const [refresh, setRefresh] = useState(0);
  const generation = useRef(0); const [loadingMore, setLoadingMore] = useState(false);
  useTitle("Token investigation dossier");
  useEffect(() => {
    let active = true; generation.current += 1; setError(null); setDossier(null); setEvents([]); setNext(null); setLoadingMore(false);
    void Promise.all([read<{ dossier: Dossier }>(apiCasePath(key)), read<{ events: CaseEvent[]; next_before: number | null }>(`${apiCasePath(key)}/history`)]).then(([result, history]) => {
      if (active) { setDossier(result.dossier); setEvents(history.events); setNext(history.next_before); }
    }).catch(error => { if (active) setError(errorText(error)); });
    return () => { active = false; };
  }, [key.chain, key.address, refresh]);
  async function older() {
    if (next === null || loadingMore) return;
    const current = generation.current; setLoadingMore(true);
    try { const history = await read<{ events: CaseEvent[]; next_before: number | null }>(`${apiCasePath(key)}/history?before=${next}`);
      if (current === generation.current) { setEvents(previous => [...previous, ...history.events]); setNext(history.next_before); }
    } catch (error) { if (current === generation.current) setError(errorText(error)); }
    finally { if (current === generation.current) setLoadingMore(false); }
  }
  return <section className="mx-auto w-full max-w-5xl space-y-8 px-6 py-12 [overflow-wrap:anywhere]">
    <Link href="/library" className="underline">Library</Link>
    <header><h1 className="display text-3xl">Token dossier</h1><p className="mt-3 font-mono text-sm break-all">{key.chain}: {key.address}</p></header>
    {error && <p role="alert">{error}</p>}
    {!error && !dossier && <p role="status">Loading dossier…</p>}
    {dossier && <><p>Updated {time(dossier.updated_at)} · revision {dossier.revision}</p>
      {dossier.assessment ? <AssessmentView assessment={dossier.assessment} /> : <p>No completed assessment has been recorded. A submitted allegation is unverified.</p>}</>}
    <h2 className="text-2xl">Investigation and correction history</h2>
    {events.map(event => <details key={event.revision} className="rounded border border-[var(--color-line)] p-4">
      <summary>{event.kind} · {time(event.at)} · revision {event.revision}</summary>
      <p className="mt-3 text-xs">Historical content retains its original observation time. Requests and contributions are unverified claims.</p>
      <pre className="mt-3 max-h-96 overflow-auto whitespace-pre-wrap break-all text-xs">{JSON.stringify(event.payload, null, 2)}</pre>
      <p className="mt-3 font-mono text-xs break-all">Checkpoint: {event.hash}</p>
      <button className={button} onClick={() => {
        const url = URL.createObjectURL(new Blob([JSON.stringify(event, null, 2)], { type: "application/json" }));
        const anchor = document.createElement("a"); anchor.href = url; anchor.download = `case-revision-${event.revision}.json`; anchor.click(); URL.revokeObjectURL(url);
      }}>Download revision</button>
    </details>)}
    {next !== null && <button disabled={loadingMore} className={button} onClick={() => void older()}>Older history</button>}
    <RequestForm key={`${key.chain}:${key.address}`} target={key} onComplete={() => setRefresh(previous => previous + 1)} />
    <Methodology />
  </section>;
}

function AssessmentView({ assessment }: { assessment: Assessment }) {
  return <div className="space-y-5"><h2 className="text-2xl">{assessment.level} · {assessment.complete ? "Requested scope covered" : "Partial investigation"}</h2>
    <p>{assessment.reply}</p>
    <div className="space-y-3">{assessment.findings.map((finding, index) => <article key={index} className="rounded border border-[var(--color-line)] p-4">
      <p className="text-xs uppercase text-[var(--color-dim)]">{finding.status} · {finding.kind}</p><p>{finding.text}</p>
      <p className="mt-2 text-xs break-all">Evidence: {finding.evidence.join(", ")}</p>
    </article>)}</div>
    <h3 className="text-xl">Sources and unresolved checks</h3>
    {assessment.observations.map(observation => <details key={observation.id} className="rounded border border-[var(--color-line)] p-3">
      <summary>{observation.kind} · {time(observation.at)} {observation.gap ? "· incomplete" : ""}</summary>
      <p className="mt-2 font-mono text-xs break-all">{observation.source} · {observation.read_point ?? "read point unavailable"}</p>
      {observation.gap && <p className="mt-2">Unresolved: {observation.gap}</p>}
      {publicSource((observation.value as { source_url?: unknown } | null)?.source_url) && <a className="underline" href={publicSource((observation.value as { source_url?: unknown }).source_url) ?? undefined} target="_blank" rel="noopener noreferrer">Source reference</a>}
      <pre className="mt-2 max-h-72 overflow-auto whitespace-pre-wrap break-all text-xs">{JSON.stringify(observation.value, null, 2)}</pre>
    </details>)}
    <details><summary>Investigation decisions and reused evidence</summary><ul className="mt-3 space-y-2">{assessment.decisions.map((decision, index) => <li key={index}>{decision}</li>)}</ul>
      <p className="mt-3 break-all text-xs">Reused references: {assessment.reused.join(", ") || "none"}</p>
      <p className="text-xs">RPC calls: {assessment.rpc_calls} · elapsed: {assessment.elapsed_ms} ms</p></details>
  </div>;
}

function RequestForm({ target, onComplete }: { target?: CaseKey; onComplete?: () => void }) {
  const [chain, setChain] = useState<Network>(target?.chain ?? "solana"); const [address, setAddress] = useState(target?.address ?? "");
  const [question, setQuestion] = useState(""); const [wallets, setWallets] = useState(""); const [transactions, setTransactions] = useState("");
  const [session, setSession] = useState<Session | null>(null); const [job, setJob] = useState<Job | null>(null);
  const [error, setError] = useState<string | null>(null); const [busy, setBusy] = useState(false);
  const idempotency = useRef<{ payload: string; key: string } | null>(null);
  useEffect(() => { let active = true; void read<Session>("/auth/me").then(session => { if (active) setSession(session.signed_in ? session : null); }).catch(() => { if (active) setSession(null); }); return () => { active = false; }; }, []);
  useEffect(() => {
    if (!job || !["pending", "running"].includes(job.status)) return;
    let active = true; const timer = window.setInterval(() => {
      void read<{ job: Job }>(`/v1/investigations/${job.request.id}`).then(result => {
        if (active) { setJob(result.job); if (!["pending", "running"].includes(result.job.status)) onComplete?.(); }
      }).catch(error => { if (active) setError(errorText(error)); });
    }, 5000);
    return () => { active = false; window.clearInterval(timer); };
  }, [job?.request.id, job?.status, onComplete]);
  async function submit(event: FormEvent) {
    event.preventDefault(); if (!session || busy) return; setBusy(true); setError(null);
    const input = { chain: target?.chain ?? chain, address: target?.address ?? address, question, wallets: wallets.split(/[\s,]+/).filter(Boolean), transactions: transactions.split(/[\s,]+/).filter(Boolean), window: null };
    const payload = JSON.stringify(input);
    if (idempotency.current?.payload !== payload) idempotency.current = { payload, key: crypto.randomUUID() };
    try {
      const result = await read<{ job: Job }>("/v1/investigations", { method: "POST", headers: { "content-type": "application/json", "x-csrf-token": session.csrf_token }, body: JSON.stringify({ ...input, idempotency_key: idempotency.current.key }) });
      setJob(result.job);
    } catch (error) { setError(errorText(error)); } finally { setBusy(false); }
  }
  return <div className="space-y-4 rounded border border-[var(--color-line)] p-5"><h2 className="text-2xl">{target ? "Build on this investigation" : "Request a free investigation"}</h2>
    <p className="text-sm">Inspect existing coverage first. Submitted questions, wallet leads and results become public. Contributions are unverified until checked; capacity is limited.</p>
    {!session ? <a className="underline" href={`${API}/auth/x/start`}>Sign in with X to contribute</a> : <p className="text-sm">Signed in as @{session.handle}</p>}
    <form className="space-y-3" onSubmit={event => void submit(event)}>
      {!target && <><label className="block">Network<select className={field} value={chain} onChange={event => setChain(event.target.value as Network)}>{networks.filter(network=>network!=="robinhood").map(network => <option key={network}>{network}</option>)}</select></label>
        <label className="block">Token address<input required maxLength={128} className={field} value={address} onChange={event => setAddress(event.target.value)} /></label>
        {address && <Link className="text-sm underline" href={casePath({ chain, address })}>Inspect existing dossier</Link>}</>}
      <label className="block">Question, allegation, new lead or correction<textarea required maxLength={4096} className={field} value={question} onChange={event => setQuestion(event.target.value)} /></label>
      <label className="block">Wallet leads (optional)<input maxLength={1024} className={field} value={wallets} onChange={event => setWallets(event.target.value)} /></label>
      <label className="block">Transaction leads (optional)<input maxLength={2048} className={field} value={transactions} onChange={event => setTransactions(event.target.value)} /></label>
      <button disabled={!session || busy} className={button}>{busy ? "Submitting…" : "Submit public request"}</button>
    </form>
    {error && <p role="alert">{error}</p>}
    {job && <div role="status"><p>Request {job.status}. {job.error}</p><Link className="underline" href={casePath(job.request.case)}>Open the resulting public dossier</Link>
      <p className="mt-2 font-mono text-xs break-all">Request ID: {job.request.id}</p></div>}
  </div>;
}

function Methodology() {
  return <aside className="space-y-3 border-t border-[var(--color-line)] pt-6 text-sm"><h2 className="text-xl">How to read these cases</h2>
    <p>A question is a submitted claim. An observation records what a specific reader obtained at a stated time. A finding references observations. Unavailable data and unrecognized mechanisms remain unresolved.</p>
    <p>Supported checks vary by network and protocol. Receiving fees does not prove undisclosed ownership; a configured allocation does not prove payment. Graduation can move liquidity to another venue.</p>
    <p>Prior cases and legitimate lookalikes guide subsequent reads. Dynamic state must be read again. Corrections preserve history and mark dependent findings for review.</p>
    <p>Downloadable checkpoints help detect changes to recorded revisions. The operator controls storage; a hash is not independent immutability. Paid investigations and universal protocol coverage are not available through this free portal.</p>
  </aside>;
}
