// SPDX-License-Identifier: Apache-2.0
//! Chain-qualified public cases and durable bounded intake (ADR 0044).
//!
//! The analyst and Library share these rows. Account identities stay in their
//! own store; actor hashes below exist only for admission, never public output.

use crate::Memory;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fmt, str::FromStr};

/// Networks whose identity is understood; capabilities are declared separately.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Network {
    /// Solana mainnet.
    Solana,
    /// Base mainnet, EVM chain 8453.
    Base,
    /// Ethereum mainnet, EVM chain 1.
    Ethereum,
    /// Historical Robinhood integration.
    Robinhood,
}

impl Network {
    /// Canonical public/storage key.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Solana => "solana",
            Self::Base => "base",
            Self::Ethereum => "ethereum",
            Self::Robinhood => "robinhood",
        }
    }
    /// Expected mainnet EVM chain id when newly supported.
    #[must_use]
    pub const fn evm_id(self) -> Option<u64> {
        match self {
            Self::Base => Some(8453),
            Self::Ethereum => Some(1),
            _ => None,
        }
    }
}

impl fmt::Display for Network {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for Network {
    type Err = CaseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "solana" => Ok(Self::Solana),
            "base" => Ok(Self::Base),
            "ethereum" => Ok(Self::Ethereum),
            "robinhood" => Ok(Self::Robinhood),
            _ => Err(CaseError::Invalid(
                "select solana, base, ethereum or robinhood".into(),
            )),
        }
    }
}

/// Address family alone never identifies an EVM network.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaseKey {
    /// Explicit network.
    pub chain: Network,
    /// Canonical mint/contract address.
    pub address: String,
}

impl CaseKey {
    /// Validate and normalize a token address without choosing a network.
    ///
    /// # Errors
    /// Invalid or unsupported input, exhausted bounds, unavailable data, or storage failure.
    pub fn new(chain: Network, address: &str) -> Result<Self, CaseError> {
        let address = if chain == Network::Solana {
            address
                .parse::<realorrug_types::Address>()
                .map_err(|e| CaseError::Invalid(e.to_string()))?
                .to_string()
        } else {
            address
                .parse::<realorrug_robinhood::Address>()
                .map_err(|e| CaseError::Invalid(e.to_string()))?
                .to_string()
        };
        Ok(Self { chain, address })
    }
    /// Stable index key, including chain.
    #[must_use]
    pub fn id(&self) -> String {
        format!("{}:{}", self.chain, self.address)
    }
}

/// Requested interval; reads may cover only a disclosed subset.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeWindow {
    /// Unix seconds, inclusive.
    pub from: u64,
    /// Unix seconds, inclusive.
    pub to: u64,
}

/// Public request data, not trusted instructions or verified evidence.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Investigation {
    /// Opaque id; caller derives it from actor and idempotency token.
    pub id: String,
    /// Token under investigation.
    pub case: CaseKey,
    /// Allegation or question, retained verbatim.
    pub question: String,
    /// Submitted account leads, distinct from the token identity.
    #[serde(default)]
    pub wallets: Vec<String>,
    /// Submitted transaction hashes/signatures.
    #[serde(default)]
    pub transactions: Vec<String>,
    /// Requested historical interval.
    pub window: Option<TimeWindow>,
    /// Public source reference; never fetched as an arbitrary URL.
    pub source: Option<String>,
    /// Platform thread identity, if supplied by the adapter.
    pub thread: Option<String>,
}

impl Investigation {
    /// Bound input and validate all typed leads before durable admission.
    ///
    /// # Errors
    /// Invalid or unsupported input, exhausted bounds, unavailable data, or storage failure.
    pub fn validate(&self) -> Result<(), CaseError> {
        if self.id.is_empty()
            || self.id.len() > 128
            || !self
                .id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
            || self.question.trim().is_empty()
            || self.question.len() > 4096
            || self.wallets.len() > 16
            || self.transactions.len() > 16
        {
            return Err(CaseError::Invalid("request exceeds input bounds".into()));
        }
        if CaseKey::new(self.case.chain, &self.case.address)? != self.case {
            return Err(CaseError::Invalid("noncanonical case address".into()));
        }
        for wallet in &self.wallets {
            CaseKey::new(self.case.chain, wallet)?;
        }
        for tx in &self.transactions {
            if self.case.chain == Network::Solana {
                tx.parse::<realorrug_types::Signature>()
                    .map_err(|e| CaseError::Invalid(e.to_string()))?;
            } else {
                tx.parse::<realorrug_robinhood::Hash32>()
                    .map_err(|e| CaseError::Invalid(e.to_string()))?;
            }
        }
        if self.window.as_ref().is_some_and(|w| w.from > w.to)
            || self.source.as_ref().is_some_and(|s| s.len() > 512)
            || self.thread.as_ref().is_some_and(|s| s.len() > 128)
        {
            return Err(CaseError::Invalid(
                "invalid window or source reference".into(),
            ));
        }
        Ok(())
    }
}

/// Reader-produced observation. Missing results carry a gap, never a zero.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Observation {
    /// Deterministic source/subject/read digest.
    pub id: String,
    /// Stable reader kind, used instead of matching display labels.
    pub kind: String,
    /// RPC method/protocol and subject reference, not credentials.
    pub source: String,
    /// Unix observation time.
    pub at: u64,
    /// Chain read point including block hash/slot when available.
    pub read_point: Option<String>,
    /// Typed reader payload. Model outputs never construct this.
    pub value: Value,
    /// Why coverage is incomplete or unavailable.
    pub gap: Option<String>,
    /// Decoder/schema version.
    pub version: String,
}

/// Code-authorized finding, with dependencies needed by corrections.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    /// Stable mechanism/rule kind.
    pub kind: String,
    /// Measured statement or explicitly labeled interpretation.
    pub text: String,
    /// Supporting observation ids.
    pub evidence: Vec<String>,
    /// Counterevidence ids.
    pub counterevidence: Vec<String>,
    /// measured / inference / unresolved / needs_review.
    pub status: String,
}

/// One retained assessment; later revisions do not erase it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Assessment {
    /// Request id.
    pub request_id: String,
    /// Code-computed verdict, not a planner recommendation.
    pub level: String,
    /// Authorized public draft.
    pub reply: String,
    /// Reader evidence and explicitly recorded gaps.
    pub observations: Vec<Observation>,
    /// Findings and dependency links.
    pub findings: Vec<Finding>,
    /// Prior observation/case references actually retrieved.
    pub reused: Vec<String>,
    /// Actual tool decisions, including why selected.
    pub decisions: Vec<String>,
    /// Calls made, including failed reads.
    pub rpc_calls: u32,
    /// Whole-request elapsed milliseconds.
    pub elapsed_ms: u64,
    /// Whether scope was fully covered.
    pub complete: bool,
}

/// Durable public request status. Actor identifiers are deliberately absent.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Job {
    /// Submitted data.
    pub request: Investigation,
    /// pending / running / completed / partial / failed.
    pub status: String,
    /// Admission time, Unix seconds.
    pub admitted_at: u64,
    /// Most recent transition.
    pub updated_at: u64,
    /// Attempts charged before dispatch.
    pub model_attempts: u32,
    /// Public recovery/failure explanation.
    pub error: Option<String>,
}

/// Append-only public history event and its operator-controlled checkpoint.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CaseEvent {
    /// Global monotonically increasing revision.
    pub revision: u64,
    /// Subject.
    pub case: CaseKey,
    /// request / assessment / contribution / correction / interrupted.
    pub kind: String,
    /// Event time.
    pub at: u64,
    /// Public event content.
    pub payload: Value,
    /// Previous event digest.
    pub previous_hash: String,
    /// Digest of this event.
    pub hash: String,
}

/// Read projection for the Library.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Dossier {
    /// Subject identity.
    pub case: CaseKey,
    /// Latest published revision.
    pub revision: u64,
    /// Latest event time.
    pub updated_at: u64,
    /// Latest assessment, with corrected dependencies marked for review.
    pub assessment: Option<Assessment>,
}

/// Small Library row; evidence is fetched only when opening the dossier.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CaseSummary {
    /// Chain-qualified token.
    pub case: CaseKey,
    /// Newest event revision.
    pub revision: u64,
    /// Newest event time.
    pub updated_at: u64,
    /// Compact latest assessment; includes no raw observations.
    pub assessment: Option<serde_json::Value>,
}

/// Domain-separated idempotency identity; the actor itself is not public.
///
/// # Panics
/// Only if the JSON serializer cannot encode a tuple of strings.
#[must_use]
pub fn request_id(actor: &str, token: &str) -> String {
    blake3::hash(
        serde_json::to_string(&("realorrug-request-v1", actor, token))
            .expect("strings serialize")
            .as_bytes(),
    )
    .to_hex()
    .to_string()
}

/// X account identity is hashed only for private cross-surface quotas.
#[must_use]
pub fn actor_id(x_id: &str) -> String {
    blake3::hash(format!("realorrug-case-actor-v1:{x_id}").as_bytes())
        .to_hex()
        .to_string()
}

/// Admission limits supplied by the operator; there is no open default.
#[derive(Clone, Copy, Debug)]
pub struct Capacity {
    /// Across X and website, per UTC day.
    pub daily: u32,
    /// Per pseudonymous actor, per UTC day.
    pub per_actor: u32,
    /// Pending plus running jobs.
    pub pending: u32,
}

/// Case persistence/admission failure.
#[derive(Debug, thiserror::Error)]
pub enum CaseError {
    /// Storage error.
    #[error("{0}")]
    Sqlite(#[from] rusqlite::Error),
    /// Schema/payload error.
    #[error("{0}")]
    Json(#[from] serde_json::Error),
    /// Invalid public input.
    #[error("{0}")]
    Invalid(String),
    /// Denied capacity, including missing configuration.
    #[error("investigation capacity unavailable")]
    Capacity,
    /// Same idempotency identity reused with different content.
    #[error("request id already names different content")]
    Conflict,
    /// Corrupted append-only checkpoint.
    #[error("case history does not verify at revision {0}")]
    Integrity(u64),
}

///
/// # Errors
/// Invalid or unsupported input, exhausted bounds, unavailable data, or storage failure.
pub(crate) fn init(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS case_jobs (
        id TEXT PRIMARY KEY, case_key TEXT NOT NULL, actor TEXT NOT NULL,
        payload TEXT NOT NULL, status TEXT NOT NULL, admitted_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL, model_attempts INTEGER NOT NULL DEFAULT 0, error TEXT);
      CREATE INDEX IF NOT EXISTS case_jobs_pending ON case_jobs(status, admitted_at);
      CREATE INDEX IF NOT EXISTS case_jobs_actor ON case_jobs(actor, admitted_at);
      CREATE TABLE IF NOT EXISTS case_events (
        revision INTEGER PRIMARY KEY AUTOINCREMENT, case_key TEXT NOT NULL,
        chain TEXT NOT NULL, address TEXT NOT NULL, kind TEXT NOT NULL,
        at INTEGER NOT NULL, payload TEXT NOT NULL, previous_hash TEXT NOT NULL UNIQUE,
        hash TEXT NOT NULL);
      CREATE INDEX IF NOT EXISTS case_events_subject ON case_events(case_key, revision);
      CREATE TRIGGER IF NOT EXISTS case_events_no_update BEFORE UPDATE ON case_events
        BEGIN SELECT RAISE(ABORT, 'case history is append-only'); END;
      CREATE TRIGGER IF NOT EXISTS case_events_no_delete BEFORE DELETE ON case_events
        BEGIN SELECT RAISE(ABORT, 'case history is append-only'); END;
      CREATE TABLE IF NOT EXISTS case_threads (thread TEXT PRIMARY KEY, case_key TEXT NOT NULL,
        chain TEXT NOT NULL, address TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS case_worker (id INTEGER PRIMARY KEY CHECK(id=1), at INTEGER NOT NULL);
      CREATE TABLE IF NOT EXISTS case_delivery (id TEXT PRIMARY KEY, mention TEXT NOT NULL,
        author TEXT NOT NULL, attempted INTEGER NOT NULL DEFAULT 0, reply_id TEXT);
      CREATE TABLE IF NOT EXISTS case_clarifications (mention TEXT PRIMARY KEY,text TEXT NOT NULL,at INTEGER NOT NULL,
        author TEXT NOT NULL,attempted INTEGER NOT NULL DEFAULT 0);
      CREATE TABLE IF NOT EXISTS case_relationships (chain TEXT NOT NULL,address TEXT NOT NULL,
        case_key TEXT NOT NULL,observation_id TEXT NOT NULL,PRIMARY KEY(chain,address,case_key,observation_id));
      CREATE INDEX IF NOT EXISTS case_relationships_address ON case_relationships(chain,address);")
}

fn digest(previous: &str, case: &str, kind: &str, at: u64, payload: &str) -> String {
    let bytes =
        serde_json::to_vec(&(previous, case, kind, at, payload)).expect("strings serialize");
    blake3::hash(&bytes).to_hex().to_string()
}

fn append(
    conn: &Connection,
    case: &CaseKey,
    kind: &str,
    at: u64,
    payload: &Value,
) -> Result<(), CaseError> {
    let previous: String = conn
        .query_row(
            "SELECT hash FROM case_events ORDER BY revision DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or_else(|| "realorrug-case-history-v1".into());
    let payload = serde_json::to_string(payload)?;
    if payload.len() > 1_048_576 {
        return Err(CaseError::Invalid("case event too large".into()));
    }
    let hash = digest(&previous, &case.id(), kind, at, &payload);
    conn.execute(
        "INSERT INTO case_events(case_key,chain,address,kind,at,payload,previous_hash,hash)
        VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
        params![
            case.id(),
            case.chain.name(),
            case.address,
            kind,
            at,
            payload,
            previous,
            hash
        ],
    )?;
    Ok(())
}

impl Memory {
    /// Record an unresolved identity or admission refusal without manufacturing a case.
    ///
    /// # Errors
    /// Invalid or unsupported input, exhausted bounds, unavailable data, or storage failure.
    pub fn remember_case_clarification(
        &self,
        mention: &str,
        text: &str,
        author: &str,
        at: u64,
        limits: Capacity,
    ) -> Result<(), CaseError> {
        self.case_transaction(|conn|{
            let count:u32=conn.query_row("SELECT count(*) FROM case_clarifications WHERE at>=?1",[at/86_400*86_400],|r|r.get(0))?;
            if count>=limits.daily{return Ok(());}
            let actor_count:u32=conn.query_row("SELECT count(*) FROM case_clarifications WHERE at>=?1 AND author=?2",params![at/86_400*86_400,author],|r|r.get(0))?;
            if actor_count>=limits.per_actor{return Ok(());}
            conn.execute("INSERT INTO case_clarifications(mention,text,author,at) VALUES(?1,?2,?3,?4) ON CONFLICT DO NOTHING",params![mention,text,author,at])?;
            Ok(())
        })
    }

    /// One unresolved identity reply, separate from fabricated token cases.
    ///
    /// # Errors
    /// Storage failure.
    pub fn next_case_clarification(&self) -> Result<Option<(String, String, String)>, CaseError> {
        Ok(self.conn.query_row("SELECT mention,text,author FROM case_clarifications WHERE attempted=0 ORDER BY at LIMIT 1",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?)
    }

    /// Durable barrier for a clarification whose external result may be unknown.
    ///
    /// # Errors
    /// Storage failure or a previously attempted clarification.
    pub fn begin_case_clarification(&self, mention: &str) -> Result<(), CaseError> {
        if self.conn.execute(
            "UPDATE case_clarifications SET attempted=1 WHERE mention=?1 AND attempted=0",
            [mention],
        )? != 1
        {
            return Err(CaseError::Conflict);
        }
        Ok(())
    }
    /// Private X routing data never appears in exported public case events.
    ///
    /// # Errors
    /// Invalid or unsupported input, exhausted bounds, unavailable data, or storage failure.
    pub fn remember_case_delivery(
        &self,
        id: &str,
        mention: &str,
        author: &str,
    ) -> Result<(), CaseError> {
        self.conn.execute(
            "INSERT INTO case_delivery(id,mention,author) VALUES (?1,?2,?3) ON CONFLICT DO NOTHING",
            params![id, mention, author],
        )?;
        Ok(())
    }

    /// Return one completed request whose publication has not been attempted.
    ///
    /// # Errors
    /// Invalid or unsupported input, exhausted bounds, unavailable data, or storage failure.
    pub fn next_case_delivery(&self) -> Result<Option<(String, String, String)>, CaseError> {
        Ok(self.conn.query_row("SELECT d.id,d.mention,d.author FROM case_delivery d JOIN case_jobs j ON j.id=d.id WHERE d.attempted=0 AND j.status IN ('completed','partial') ORDER BY j.updated_at,d.id LIMIT 1", [], |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?)
    }

    /// Durable no-repeat barrier before any publication attempt.
    ///
    /// # Errors
    /// Invalid or unsupported input, exhausted bounds, unavailable data, or storage failure.
    pub fn begin_case_delivery(&self, id: &str) -> Result<(), CaseError> {
        if self.conn.execute(
            "UPDATE case_delivery SET attempted=1 WHERE id=?1 AND attempted=0",
            [id],
        )? != 1
        {
            return Err(CaseError::Conflict);
        }
        Ok(())
    }

    /// Link an accepted publication to the exact request, without public identity data.
    ///
    /// # Errors
    /// Invalid or unsupported input, exhausted bounds, unavailable data, or storage failure.
    pub fn finish_case_delivery(
        &self,
        request: &Investigation,
        at: u64,
        reply_id: &str,
    ) -> Result<(), CaseError> {
        self.case_transaction(|conn| {
            conn.execute(
                "UPDATE case_delivery SET reply_id=?2 WHERE id=?1 AND attempted=1",
                params![request.id, reply_id],
            )?;
            append(
                conn,
                &request.case,
                "publication",
                at,
                &serde_json::json!({"request_id":request.id,"reply_id":reply_id}),
            )
        })
    }

    /// Find the immutable assessment for a request rather than a newer dossier.
    ///
    /// # Errors
    /// Invalid or unsupported input, exhausted bounds, unavailable data, or storage failure.
    pub fn case_assessment(
        &self,
        request: &Investigation,
    ) -> Result<Option<Assessment>, CaseError> {
        let mut stmt=self.conn.prepare("SELECT payload FROM case_events WHERE case_key=?1 AND kind='assessment' ORDER BY revision DESC")?;
        let rows = stmt.query_map([request.case.id()], |r| r.get::<_, String>(0))?;
        for row in rows {
            let assessment: Assessment = serde_json::from_str(&row?)?;
            if assessment.request_id == request.id {
                return Ok(Some(assessment));
            }
        }
        Ok(None)
    }

    /// Bounded same-network cases sharing a reader-established address. This is
    /// an investigative lead, never an ownership or common-controller assertion.
    ///
    /// # Errors
    /// Invalid or unsupported input, exhausted bounds, unavailable data, or storage failure.
    pub fn related_cases(
        &self,
        case: &CaseKey,
        subjects: &[String],
    ) -> Result<Vec<Dossier>, CaseError> {
        let mut cases = Vec::new();
        let mut keys = Vec::new();
        for subject in subjects.iter().take(17) {
            let mut stmt=self.conn.prepare("SELECT DISTINCT e.chain,e.address FROM case_relationships r JOIN case_events e ON e.case_key=r.case_key WHERE r.chain=?1 AND r.address=?2 AND r.case_key!=?3 LIMIT 3")?;
            let rows = stmt.query_map(params![case.chain.name(), subject, case.id()], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            for row in rows {
                let (chain, address) = row?;
                let key = CaseKey::new(chain.parse()?, &address)?;
                if keys.contains(&key.id()) {
                    continue;
                }
                if let Some(dossier) = self.case_dossier(&key)? {
                    keys.push(key.id());
                    cases.push(dossier);
                    if cases.len() == 3 {
                        return Ok(cases);
                    }
                }
            }
        }
        Ok(cases)
    }
    fn case_transaction<T>(
        &self,
        f: impl FnOnce(&Connection) -> Result<T, CaseError>,
    ) -> Result<T, CaseError> {
        self.conn.execute_batch("BEGIN IMMEDIATE")?;
        match f(&self.conn) {
            Ok(value) => {
                self.conn.execute_batch("COMMIT")?;
                Ok(value)
            }
            Err(e) => {
                self.conn.execute_batch("ROLLBACK")?;
                Err(e)
            }
        }
    }

    /// Append and admit atomically; duplicate requests bypass quotas, never work.
    ///
    /// # Errors
    /// Invalid or unsupported input, exhausted bounds, unavailable data, or storage failure.
    pub fn enqueue_case(
        &self,
        request: &Investigation,
        actor: &str,
        at: u64,
        limits: Capacity,
    ) -> Result<Job, CaseError> {
        request.validate()?;
        self.case_transaction(|conn| {
            if let Some(job) = self.case_job(&request.id)? {
                if job.request != *request { return Err(CaseError::Conflict); }
                return Ok(job);
            }
            if let Some(thread) = &request.thread
                && self.case_for_thread(thread)?.is_some_and(|k| k != request.case) {
                    return Err(CaseError::Invalid("this thread already names another token; use a new thread or an explicit website request".into()));
                }
            let today = at / 86_400 * 86_400;
            let count: u32 = conn.query_row("SELECT count(*) FROM case_jobs WHERE admitted_at>=?1",
                [today], |r| r.get(0))?;
            let own: u32 = conn.query_row(
                "SELECT count(*) FROM case_jobs WHERE actor=?1 AND admitted_at>=?2",
                params![actor,today], |r| r.get(0))?;
            let pending: u32 = conn.query_row(
                "SELECT count(*) FROM case_jobs WHERE status IN ('pending','running')", [], |r| r.get(0))?;
            if count >= limits.daily || own >= limits.per_actor || pending >= limits.pending {
                return Err(CaseError::Capacity);
            }
            conn.execute("INSERT INTO case_jobs(id,case_key,actor,payload,status,admitted_at,updated_at)
                VALUES(?1,?2,?3,?4,'pending',?5,?5)",
                params![request.id,request.case.id(),actor,serde_json::to_string(request)?,at])?;
            append(conn, &request.case, "request", at, &serde_json::to_value(request)?)?;
            if let Some(thread) = &request.thread {
                conn.execute("INSERT INTO case_threads(thread,case_key,chain,address) VALUES(?1,?2,?3,?4)
                    ON CONFLICT(thread) DO UPDATE SET case_key=excluded.case_key,
                    chain=excluded.chain,address=excluded.address",
                    params![thread,request.case.id(),request.case.chain.name(),request.case.address])?;
            }
            self.case_job(&request.id)?.ok_or(CaseError::Conflict)
        })
    }

    /// Request status contains no actor hash or account identity.
    ///
    /// # Errors
    /// Invalid or unsupported input, exhausted bounds, unavailable data, or storage failure.
    pub fn case_job(&self, id: &str) -> Result<Option<Job>, CaseError> {
        let row = self
            .conn
            .query_row(
                "SELECT payload,status,admitted_at,updated_at,model_attempts,error
            FROM case_jobs WHERE id=?1",
                [id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                    ))
                },
            )
            .optional()?;
        row.map(
            |(payload, status, admitted_at, updated_at, model_attempts, error)| {
                Ok(Job {
                    request: serde_json::from_str(&payload)?,
                    status,
                    admitted_at,
                    updated_at,
                    model_attempts,
                    error,
                })
            },
        )
        .transpose()
    }

    /// Persisted thread target survives process restarts.
    ///
    /// # Errors
    /// Invalid or unsupported input, exhausted bounds, unavailable data, or storage failure.
    pub fn case_for_thread(&self, thread: &str) -> Result<Option<CaseKey>, CaseError> {
        let row = self
            .conn
            .query_row(
                "SELECT chain,address FROM case_threads WHERE thread=?1",
                [thread],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )
            .optional()?;
        row.map(|(chain, address)| CaseKey::new(chain.parse()?, &address))
            .transpose()
    }

    /// Claim one pending job; concurrent workers cannot claim it twice.
    ///
    /// # Errors
    /// Invalid or unsupported input, exhausted bounds, unavailable data, or storage failure.
    pub fn next_case_job(&self, at: u64) -> Result<Option<Job>, CaseError> {
        self.case_transaction(|conn| {
            let id: Option<String> = conn.query_row(
                "SELECT id FROM case_jobs WHERE status='pending' ORDER BY admitted_at,id LIMIT 1",
                [], |r| r.get(0)).optional()?;
            let Some(id) = id else {
                return Ok(None);
            };
            conn.execute(
                "UPDATE case_jobs SET status='running',updated_at=?2 WHERE id=?1",
                params![id, at],
            )?;
            self.case_job(&id)
        })
    }

    /// Count attempts before provider dispatch. Unknown-cost attempts stay spent.
    ///
    /// # Errors
    /// Invalid or unsupported input, exhausted bounds, unavailable data, or storage failure.
    pub fn case_model_attempt(&self, id: &str, at: u64) -> Result<(), CaseError> {
        let changed = self.conn.execute(
            "UPDATE case_jobs SET model_attempts=model_attempts+1,
            updated_at=?2 WHERE id=?1 AND status='running' AND model_attempts<3",
            params![id, at],
        )?;
        if changed != 1 {
            return Err(CaseError::Capacity);
        }
        Ok(())
    }

    /// Save result and terminal status in the same transaction before publication.
    ///
    /// # Errors
    /// Invalid or unsupported input, exhausted bounds, unavailable data, or storage failure.
    pub fn finish_case(
        &self,
        request: &Investigation,
        result: &Assessment,
        at: u64,
    ) -> Result<(), CaseError> {
        if result.request_id != request.id {
            return Err(CaseError::Conflict);
        }
        self.case_transaction(|conn| {
            let changed = conn.execute("UPDATE case_jobs SET status=?2,updated_at=?3
                WHERE id=?1 AND status='running'", params![request.id,
                if result.complete { "completed" } else { "partial" },at])?;
            if changed != 1 { return Err(CaseError::Conflict); }
            for observation in result.observations.iter().take(24){
                if let Some(addresses)=observation.value["related"].as_array(){for address in addresses.iter().take(16){
                    if let Some(address)=address.as_str()&& let Ok(key)=CaseKey::new(request.case.chain,address){
                        conn.execute("INSERT OR IGNORE INTO case_relationships(chain,address,case_key,observation_id) VALUES(?1,?2,?3,?4)",params![key.chain.name(),key.address,request.case.id(),observation.id])?;
                    }
                }}
            }
            append(conn, &request.case, "assessment", at, &serde_json::to_value(result)?)
        })
    }

    /// Restart does not retry unknown-effect work automatically.
    ///
    /// # Errors
    /// Invalid or unsupported input, exhausted bounds, unavailable data, or storage failure.
    pub fn recover_cases(&self, at: u64) -> Result<u32, CaseError> {
        self.case_transaction(|conn| {
            let mut stmt = conn.prepare("SELECT payload FROM case_jobs WHERE status='running'")?;
            let payloads = stmt.query_map([], |r| r.get::<_,String>(0))?
                .collect::<Result<Vec<_>,_>>()?;
            for payload in &payloads {
                let request: Investigation = serde_json::from_str(payload)?;
                append(conn, &request.case, "interrupted", at, &serde_json::json!({
                    "request_id":request.id,"gap":"worker interrupted; no automatic repeat",
                }))?;
            }
            let changed = conn.execute("UPDATE case_jobs SET status='failed',updated_at=?1,
                error='worker interrupted; submit a new request to continue' WHERE status='running'", [at])?;
            u32::try_from(changed).map_err(|_| CaseError::Capacity)
        })
    }

    /// Worker heartbeat makes website admission fail closed when worker is absent.
    ///
    /// # Errors
    /// Invalid or unsupported input, exhausted bounds, unavailable data, or storage failure.
    pub fn case_heartbeat(&self, at: u64) -> Result<(), CaseError> {
        self.conn.execute(
            "INSERT INTO case_worker(id,at) VALUES(1,?1)
            ON CONFLICT(id) DO UPDATE SET at=excluded.at",
            [at],
        )?;
        Ok(())
    }

    /// Heartbeat is a liveness signal, not a guarantee a job can complete.
    ///
    /// # Errors
    /// Invalid or unsupported input, exhausted bounds, unavailable data, or storage failure.
    pub fn case_worker_live(&self, at: u64) -> Result<bool, CaseError> {
        let last: Option<u64> = self
            .conn
            .query_row("SELECT at FROM case_worker WHERE id=1", [], |r| r.get(0))
            .optional()?;
        Ok(last.is_some_and(|last| last <= at && at - last < 240))
    }

    /// Paginated public history, with actor identities excluded.
    ///
    /// # Errors
    /// Invalid or unsupported input, exhausted bounds, unavailable data, or storage failure.
    pub fn case_history(
        &self,
        case: &CaseKey,
        before: u64,
        limit: u32,
    ) -> Result<Vec<CaseEvent>, CaseError> {
        let mut stmt = self.conn.prepare(
            "SELECT revision,kind,at,payload,previous_hash,hash
            FROM case_events WHERE case_key=?1 AND revision<?2 ORDER BY revision DESC LIMIT ?3",
        )?;
        let rows = stmt
            .query_map(
                params![case.id(), before.min(i64::MAX as u64), limit.min(100)],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get::<_, String>(1)?,
                        r.get(2)?,
                        r.get::<_, String>(3)?,
                        r.get(4)?,
                        r.get(5)?,
                    ))
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|(revision, kind, at, payload, previous_hash, hash)| {
                Ok(CaseEvent {
                    revision,
                    case: case.clone(),
                    kind,
                    at,
                    payload: serde_json::from_str(&payload)?,
                    previous_hash,
                    hash,
                })
            })
            .collect()
    }

    /// Current assessment with corrections applied only to the projection.
    ///
    /// # Errors
    /// Invalid or unsupported input, exhausted bounds, unavailable data, or storage failure.
    pub fn case_dossier(&self, case: &CaseKey) -> Result<Option<Dossier>, CaseError> {
        let newest = self.case_history(case, u64::MAX, 1)?.pop();
        let Some(newest) = newest else {
            return Ok(None);
        };
        let row: Option<(u64, String)> = self
            .conn
            .query_row(
                "SELECT revision,payload FROM case_events WHERE case_key=?1 AND kind='assessment'
             ORDER BY revision DESC LIMIT 1",
                [case.id()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let mut assessment = row
            .as_ref()
            .map(|(_, p)| serde_json::from_str::<Assessment>(p))
            .transpose()?;
        if let (Some((_revision, _)), Some(a)) = (row, &mut assessment) {
            let invalid = self.case_invalidations(case, 0)?;
            for finding in &mut a.findings {
                if finding.evidence.iter().any(|id| invalid.contains(id)) {
                    finding.status = "needs_review".into();
                    a.complete = false;
                }
            }
            if a.findings
                .iter()
                .any(|finding| finding.status == "needs_review")
            {
                a.level = "CantTell".into();
                a.reply = "Prior evidence was challenged; reassessment is required.".into();
            }
        }
        Ok(Some(Dossier {
            case: case.clone(),
            revision: newest.revision,
            updated_at: newest.at,
            assessment,
        }))
    }

    fn case_invalidations(&self, case: &CaseKey, after: u64) -> Result<Vec<String>, CaseError> {
        let mut stmt = self.conn.prepare(
            "SELECT payload FROM case_events
            WHERE case_key=?1 AND kind='correction' AND revision>?2",
        )?;
        let payloads = stmt
            .query_map(params![case.id(), after], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let mut ids = Vec::new();
        for payload in payloads {
            let value: Value = serde_json::from_str(&payload)?;
            if let Some(id) = value["observation_id"].as_str() {
                ids.push(id.to_owned());
            }
        }
        Ok(ids)
    }

    /// Public contribution remains unverified. Operator-verified corrections use
    /// `correct_case`; website callers cannot invalidate measured evidence.
    ///
    /// # Errors
    /// Invalid or unsupported input, exhausted bounds, unavailable data, or storage failure.
    pub fn contribute_case(&self, case: &CaseKey, at: u64, note: &str) -> Result<(), CaseError> {
        if note.trim().is_empty() || note.len() > 4096 {
            return Err(CaseError::Invalid("invalid contribution".into()));
        }
        self.case_transaction(|conn| {
            append(
                conn,
                case,
                "contribution",
                at,
                &serde_json::json!({"note":note,"status":"unverified"}),
            )
        })
    }

    /// Operator/verified-reader path only: append invalidation, never edit history.
    ///
    /// # Errors
    /// Invalid or unsupported input, exhausted bounds, unavailable data, or storage failure.
    pub fn correct_case(
        &self,
        case: &CaseKey,
        at: u64,
        observation_id: &str,
        why: &str,
    ) -> Result<(), CaseError> {
        if why.is_empty() || why.len() > 4096 {
            return Err(CaseError::Invalid("invalid correction".into()));
        }
        let exists = self
            .case_history(case, u64::MAX, 100)?
            .iter()
            .filter(|event| event.kind == "assessment")
            .any(|event| {
                event.payload["observations"]
                    .as_array()
                    .is_some_and(|observations| {
                        observations
                            .iter()
                            .any(|o| o["id"].as_str() == Some(observation_id))
                    })
            });
        if !exists {
            return Err(CaseError::Invalid("observation is not in the most recent retained case assessments; select a recorded observation".into()));
        }
        self.case_transaction(|conn| {
            conn.execute(
                "DELETE FROM case_relationships WHERE case_key=?1 AND observation_id=?2",
                params![case.id(), observation_id],
            )?;
            append(
                conn,
                case,
                "correction",
                at,
                &serde_json::json!({"observation_id":observation_id,"why":why}),
            )
        })
    }

    /// Compact, paginated index; no table-wide dossier hydration.
    ///
    /// # Errors
    /// Invalid or unsupported input, exhausted bounds, unavailable data, or storage failure.
    pub fn library_cases(
        &self,
        chain: Option<Network>,
        query: &str,
        before: u64,
        limit: u32,
    ) -> Result<Vec<CaseSummary>, CaseError> {
        if query.len() > 128 {
            return Err(CaseError::Invalid("search too long".into()));
        }
        let mut stmt = self.conn.prepare("SELECT chain,address,max(revision) AS newest,max(at),
            (SELECT CASE WHEN EXISTS (
                SELECT 1 FROM case_events c, json_each(a.payload,'$.findings') f,
                  json_each(f.value,'$.evidence') e
                WHERE c.case_key=a.case_key AND c.kind='correction'
                  AND e.value=json_extract(c.payload,'$.observation_id'))
              THEN json_object('level','CantTell','complete',0)
              ELSE json_object('level',json_extract(a.payload,'$.level'),'complete',json_extract(a.payload,'$.complete')) END
              FROM case_events a WHERE a.case_key=case_events.case_key AND a.kind='assessment' ORDER BY a.revision DESC LIMIT 1)
            FROM case_events
            WHERE (?1 IS NULL OR chain=?1) AND instr(lower(address),lower(?2))>0
            GROUP BY case_key HAVING newest<?3 ORDER BY newest DESC LIMIT ?4")?;
        let keys = stmt
            .query_map(
                params![
                    chain.map(Network::name),
                    query,
                    before.min(i64::MAX as u64),
                    limit.min(50)
                ],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, u64>(2)?,
                        r.get::<_, u64>(3)?,
                        r.get::<_, Option<String>>(4)?,
                    ))
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;
        keys.into_iter()
            .map(|(chain, address, revision, updated_at, assessment)| {
                let mut assessment = assessment
                    .map(|json| serde_json::from_str::<Value>(&json))
                    .transpose()?;
                if let Some(assessment) = &mut assessment {
                    assessment["complete"] =
                        Value::Bool(assessment["complete"].as_u64() == Some(1));
                }
                Ok(CaseSummary {
                    case: CaseKey::new(chain.parse()?, &address)?,
                    revision,
                    updated_at,
                    assessment,
                })
            })
            .collect()
    }

    /// Verify all append-only events before backup/review; custody is still local.
    ///
    /// # Errors
    /// Invalid or unsupported input, exhausted bounds, unavailable data, or storage failure.
    pub fn verify_cases(&self) -> Result<String, CaseError> {
        let mut stmt = self.conn.prepare(
            "SELECT revision,case_key,kind,at,payload,previous_hash,hash
            FROM case_events ORDER BY revision",
        )?;
        let mut rows = stmt.query([])?;
        let mut previous = "realorrug-case-history-v1".to_owned();
        while let Some(row) = rows.next()? {
            let revision: u64 = row.get(0)?;
            let linked: String = row.get(5)?;
            let hash: String = row.get(6)?;
            if linked != previous
                || hash
                    != digest(
                        &previous,
                        &row.get::<_, String>(1)?,
                        &row.get::<_, String>(2)?,
                        row.get(3)?,
                        &row.get::<_, String>(4)?,
                    )
            {
                return Err(CaseError::Integrity(revision));
            }
            previous = hash;
        }
        Ok(previous)
    }

    /// Consistent SQLite snapshot; destination must not already exist.
    ///
    /// # Errors
    /// Invalid or unsupported input, exhausted bounds, unavailable data, or storage failure.
    pub fn backup_cases(&self, destination: &std::path::Path) -> Result<String, CaseError> {
        self.verify_cases()?;
        if destination.exists() {
            return Err(CaseError::Invalid("backup destination exists".into()));
        }
        self.conn
            .execute("VACUUM INTO ?1", [destination.to_string_lossy().as_ref()])?;
        Memory::read_only(destination)
            .map_err(|e| CaseError::Invalid(e.to_string()))?
            .verify_cases()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(chain: Network, id: &str) -> Investigation {
        Investigation {
            id: id.into(),
            case: CaseKey::new(chain, "0x1111111111111111111111111111111111111111").unwrap(),
            question: "Where do the fees go?".into(),
            wallets: vec![],
            transactions: vec![],
            window: None,
            source: None,
            thread: Some("thread-a".into()),
        }
    }
    #[test]
    fn cases_survive_restart_isolate_networks_and_do_not_repeat_interrupted_work() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("memory.sqlite3");
        let limits = Capacity {
            daily: 3,
            per_actor: 2,
            pending: 2,
        };
        let a = request(Network::Base, "a");
        let mut b = request(Network::Ethereum, "b");
        b.thread = Some("thread-b".into());
        {
            let m = Memory::open(&path).unwrap();
            m.enqueue_case(&a, "actor", 1, limits).unwrap();
            assert_eq!(
                m.enqueue_case(&a, "actor", 1, limits).unwrap().status,
                "pending"
            );
            let mut collision = a.clone();
            collision.question = "different".into();
            assert!(matches!(
                m.enqueue_case(&collision, "actor", 1, limits),
                Err(CaseError::Conflict)
            ));
            m.enqueue_case(&b, "actor", 2, limits).unwrap();
            assert_eq!(m.library_cases(None, "", u64::MAX, 50).unwrap().len(), 2);
            assert_eq!(m.next_case_job(3).unwrap().unwrap().request.id, "a");
            m.case_model_attempt("a", 3).unwrap();
        }
        let m = Memory::open(&path).unwrap();
        assert_eq!(m.recover_cases(4).unwrap(), 1);
        assert_eq!(m.case_job("a").unwrap().unwrap().model_attempts, 1);
        assert_eq!(m.next_case_job(5).unwrap().unwrap().request.id, "b");
        assert_eq!(
            m.case_for_thread("thread-a").unwrap().unwrap().chain,
            Network::Base
        );
        m.verify_cases().unwrap();
        let backup = dir.path().join("backup.sqlite3");
        m.backup_cases(&backup).unwrap();
        assert_eq!(
            Memory::open(&backup).unwrap().verify_cases().unwrap(),
            m.verify_cases().unwrap()
        );
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "one restart/correction lifecycle exercises the custody boundary"
    )]
    fn corrections_remove_leads_preserve_history_and_publication_attempts_survive_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cases.sqlite3");
        let memory = Memory::open(&path).unwrap();
        let limits = Capacity {
            daily: 4,
            per_actor: 4,
            pending: 4,
        };
        let a = request(Network::Base, "a");
        memory.enqueue_case(&a, "actor-private", 1, limits).unwrap();
        memory.next_case_job(2).unwrap();
        let read = crate::investigation::Read {
            tool: crate::investigation::Tool::Fees,
            subject: a.case.address.clone(),
            why: "fixture".into(),
        };
        let observation = crate::investigation::observed(
            &a.case,
            &read,
            2,
            Some("synthetic block".into()),
            serde_json::json!({"related":["0x2222222222222222222222222222222222222222"],"statements":[{"text":"Configured route; beneficiary unresolved."}]}),
            None,
        );
        let assessment = Assessment {
            request_id: a.id.clone(),
            level: "NothingUglyYet".into(),
            reply: "Configured route; beneficiary unresolved.".into(),
            observations: vec![observation.clone()],
            findings: vec![Finding {
                kind: "fees".into(),
                text: "Configured route; beneficiary unresolved.".into(),
                evidence: vec![observation.id.clone()],
                counterevidence: vec![],
                status: "measured".into(),
            }],
            reused: vec![],
            decisions: vec![],
            rpc_calls: 1,
            elapsed_ms: 1,
            complete: true,
        };
        memory.finish_case(&a, &assessment, 3).unwrap();
        assert_eq!(
            memory.library_cases(None, "", u64::MAX, 10).unwrap()[0]
                .assessment
                .as_ref()
                .unwrap()["complete"],
            true
        );
        let other =
            CaseKey::new(Network::Base, "0x3333333333333333333333333333333333333333").unwrap();
        let subjects = vec!["0x2222222222222222222222222222222222222222".into()];
        assert_eq!(memory.related_cases(&other, &subjects).unwrap().len(), 1);
        let checkpoint = memory.verify_cases().unwrap();
        memory
            .correct_case(&a.case, 4, &observation.id, "decoder was challenged")
            .unwrap();
        let summary = memory
            .library_cases(None, "", u64::MAX, 10)
            .unwrap()
            .pop()
            .unwrap()
            .assessment
            .unwrap();
        assert_eq!(summary["complete"], false);
        assert_eq!(summary["level"], "CantTell");
        assert!(memory.related_cases(&other, &subjects).unwrap().is_empty());
        let dossier = memory.case_dossier(&a.case).unwrap().unwrap();
        assert_eq!(
            dossier.assessment.unwrap().findings[0].status,
            "needs_review"
        );
        assert_ne!(checkpoint, memory.verify_cases().unwrap());
        assert_eq!(
            memory
                .case_history(&a.case, u64::MAX, 10)
                .unwrap()
                .iter()
                .filter(|e| e.kind == "assessment")
                .count(),
            1
        );
        assert!(
            memory
                .conn
                .execute("UPDATE case_events SET payload='{}'", [])
                .is_err()
        );
        memory
            .remember_case_delivery("a", "mention", "private-author")
            .unwrap();
        assert!(memory.next_case_delivery().unwrap().is_some());
        memory.begin_case_delivery("a").unwrap();
        drop(memory);
        let memory = Memory::open(&path).unwrap();
        assert!(memory.next_case_delivery().unwrap().is_none());
        let public =
            serde_json::to_string(&memory.case_history(&a.case, u64::MAX, 10).unwrap()).unwrap();
        assert!(!public.contains("private-author"));
        assert!(!public.contains("actor-private"));
        let backup = dir.path().join("backup.sqlite3");
        let head = memory.backup_cases(&backup).unwrap();
        assert_eq!(
            Memory::read_only(&backup).unwrap().verify_cases().unwrap(),
            head
        );
    }
}
