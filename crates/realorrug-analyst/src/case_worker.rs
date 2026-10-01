// SPDX-License-Identifier: Apache-2.0
//! Durable case work, independent of whether an X poll returns mentions.
use crate::{
    Cost, Mention, Spend,
    admission::{Admitted, Gate},
    case_request::{Resolved, resolve},
    daemon::Paths,
    investigator::{Metered, capacity_from, investigate},
    publish::Publisher,
};
use realorrug_model::Provider;
use realorrug_onchain::{
    Memory, RpcClient,
    cases::{Network, actor_id},
    investigation::LiveReader,
};
use std::path::Path;

fn env(key: &str) -> Option<String> {
    std::env::var(key).ok()
}

/// OS lock releases after a crash; overlapping workers must not share a meter.
///
/// # Errors
/// Another worker holds the file, or local storage is unavailable.
pub(crate) fn lease(paths: &Paths) -> Result<Option<std::fs::File>, String> {
    if capacity_from(&env).is_none() {
        return Ok(None);
    }
    // Legacy startup tolerates a corrupt ledger. The new worker must not gain
    // a fresh allowance from an unreadable existing file.
    match std::fs::read(&paths.ledger) {
        Ok(bytes) => {
            serde_json::from_slice::<realorrug_provider::Ledger>(&bytes)
                .map_err(|e| format!("existing spend ledger is invalid: {e}"))?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("existing spend ledger is unreadable: {e}")),
    }
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(format!("{}.worker.lock", paths.memory))
        .map_err(|e| e.to_string())?;
    file.try_lock()
        .map_err(|e| format!("case worker lease unavailable: {e}"))?;
    Ok(Some(file))
}

/// Verify history and quarantine interrupted work once, before accepting work.
///
/// # Errors
/// Invalid or unsupported input, exhausted bounds, unavailable data, or storage failure.
pub fn initialize(paths: &Paths) -> Result<(), String> {
    if capacity_from(&env).is_none() {
        return Ok(());
    }
    let memory = Memory::open(Path::new(&paths.memory)).map_err(|e| e.to_string())?;
    memory.verify_cases().map_err(|e| e.to_string())?;
    memory
        .recover_cases(crate::daemon::now())
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Preserve the exact allegation before the legacy ticker-only path sees it.
///
/// # Errors
/// Invalid or unsupported input, exhausted bounds, unavailable data, or storage failure.
pub fn intake(
    mention: &Mention,
    memory: &Memory,
    gate: &mut Gate,
    at: u64,
) -> Result<bool, String> {
    let Some(capacity) = capacity_from(&env) else {
        return Ok(false);
    };
    let standing = mention
        .conversation
        .as_deref()
        .map(|id| memory.case_for_thread(id))
        .transpose()
        .map_err(|e| e.to_string())?
        .flatten();
    match resolve(mention, standing.as_ref()) {
        Resolved::Nothing => Ok(false),
        Resolved::Clarify(text) => {
            if !matches!(
                gate.admit(
                    &mention.author,
                    &format!("clarify:{}", mention.id),
                    mention.conversation.as_deref(),
                    at
                ),
                Admitted::Yes
            ) {
                return Ok(true);
            }
            // A clarification has no token identity and performs no chain/model call.
            // Record it locally; publishing remains behind the owner review gate.
            eprintln!(
                "realorrug-investigator: clarification for {}: {text}",
                mention.id
            );
            memory
                .remember_case_clarification(&mention.id, &text, &mention.author, at, capacity)
                .map_err(|e| e.to_string())?;
            Ok(true)
        }
        Resolved::Ready(request) => {
            if request.case.chain == Network::Robinhood {
                return Ok(false);
            }
            if memory
                .case_job(&request.id)
                .map_err(|e| e.to_string())?
                .is_none()
                && !matches!(
                    gate.admit(
                        &mention.author,
                        &request.id,
                        mention.conversation.as_deref(),
                        at
                    ),
                    Admitted::Yes
                )
            {
                return Ok(true);
            }
            match memory.enqueue_case(&request, &actor_id(&mention.author), at, capacity) {
                Ok(_) => memory
                    .remember_case_delivery(&request.id, &mention.id, &mention.author)
                    .map_err(|e| e.to_string())?,
                Err(realorrug_onchain::cases::CaseError::Capacity) => {
                    memory
                        .remember_case_clarification(
                            &mention.id,
                            "Investigation capacity exhausted; no investigation was admitted.",
                            &mention.author,
                            at,
                            capacity,
                        )
                        .map_err(|e| e.to_string())?;
                }
                Err(
                    realorrug_onchain::cases::CaseError::Invalid(_)
                    | realorrug_onchain::cases::CaseError::Conflict,
                ) => {
                    memory.remember_case_clarification(&mention.id,"This thread or request already identifies another recorded case. Start a new request with one token address and its network.",&mention.author,at,capacity).map_err(|e|e.to_string())?;
                }
                Err(e) => return Err(e.to_string()),
            }
            Ok(true)
        }
    }
}

/// One serialized worker uses the same meter as the existing agent lanes.
///
/// # Errors
/// Invalid or unsupported input, exhausted bounds, unavailable data, or storage failure.
pub fn tick(
    paths: &Paths,
    solana: &RpcClient,
    robinhood: Option<&realorrug_robinhood::Rpc>,
    provider: Option<&dyn Provider>,
    spend: &mut Spend,
    publisher: &dyn Publisher,
) -> Result<(), String> {
    if capacity_from(&env).is_none() {
        return Ok(());
    }
    let memory = Memory::open(Path::new(&paths.memory)).map_err(|e| e.to_string())?;
    let at = crate::daemon::now();
    memory.case_heartbeat(at).map_err(|e| e.to_string())?;
    if let Some(job) = memory.next_case_job(at).map_err(|e| e.to_string())? {
        let base = env("REALORRUG_BASE_RPC")
            .filter(|v| !v.trim().is_empty())
            .map(realorrug_robinhood::Rpc::new);
        let ethereum = env("REALORRUG_ETHEREUM_RPC")
            .filter(|v| !v.trim().is_empty())
            .map(realorrug_robinhood::Rpc::new);
        let mut reader = LiveReader {
            solana,
            base: base.as_ref(),
            ethereum: ethereum.as_ref(),
            robinhood,
        };
        let result = if let Some(provider) = provider {
            let mut model = Metered {
                provider,
                spend,
                memory: &memory,
                job: &job.request.id,
                at,
                storage_failure: None,
            };
            let result = investigate(
                &job.request,
                &mut reader,
                Some(&mut model),
                Some(&memory),
                at,
            );
            if let Some(error) = model.storage_failure {
                return Err(error);
            }
            result
        } else {
            investigate(&job.request, &mut reader, None, Some(&memory), at)
        };
        memory
            .finish_case(&job.request, &result, crate::daemon::now())
            .map_err(|e| e.to_string())?;
    }
    deliver(&memory, paths, spend, publisher, at)
}

fn deliver(
    memory: &Memory,
    paths: &Paths,
    spend: &mut Spend,
    publisher: &dyn Publisher,
    at: u64,
) -> Result<(), String> {
    if env("REALORRUG_INVESTIGATOR_PUBLISH").as_deref() != Some("1")
        || publisher.name() == "dry-run"
    {
        return Ok(());
    }
    let Some((id, mention, author)) = memory.next_case_delivery().map_err(|e| e.to_string())?
    else {
        return deliver_clarification(memory, paths, spend, publisher, at);
    };
    let job = memory
        .case_job(&id)
        .map_err(|e| e.to_string())?
        .ok_or("delivery has no job")?;
    let assessment = memory
        .case_assessment(&job.request)
        .map_err(|e| e.to_string())?
        .ok_or("delivery has no assessment")?;
    let origin = env("REALORRUG_PUBLIC_ORIGIN").ok_or("public dossier origin is missing")?;
    if !valid_origin(&origin) {
        return Err(
            "public dossier origin must be an HTTPS origin without path, credentials or query"
                .into(),
        );
    }
    let link = format!(
        "{origin}/library/{}/{}",
        job.request.case.chain, job.request.case.address
    );
    let prefix = "Can't tell from these bounded reads.";
    let reply = assessment
        .findings
        .iter()
        .filter(|finding| finding.status == "measured")
        .map(|finding| format!("{prefix} {} {link}", finding.text))
        .find(|text| text.chars().count() <= 280)
        .unwrap_or_else(|| format!("{prefix} Measured evidence and missing checks: {link}"));
    let mut journal =
        realorrug_journal::Journal::open(&paths.journal).map_err(|e| e.to_string())?;
    let reserved = spend
        .authorize(Cost::Reply, at / 86_400)
        .map_err(|e| e.to_string())?;
    spend.save().map_err(|e| e.to_string())?;
    memory.begin_case_delivery(&id).map_err(|e| e.to_string())?;
    let entry = crate::log::Entry {
        at,
        mention_id: mention,
        summoner: author,
        mint: None,
        read_at: None,
        read_at_slot: None,
        fact_sheet: serde_json::to_string(&assessment).map_err(|e| e.to_string())?,
        reply,
        fellback: Some("case evidence pointer; chain identity is in the dossier URL".into()),
        refused: None,
        reply_id: None,
        signals: None,
        pointed_at: None,
        level: None,
        leads: None,
    };
    let result = crate::publish::publish(publisher, &paths.log, &mut journal, entry);
    // An uncertain external effect consumes the reservation and is never retried.
    let charged = reserved.reserved();
    spend.settle(reserved, charged);
    spend.save().map_err(|e| e.to_string())?;
    if let Some(reply_id) = result.map_err(|e| e.to_string())?.reply_id {
        memory
            .finish_case_delivery(&job.request, at, &reply_id)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn deliver_clarification(
    memory: &Memory,
    paths: &Paths,
    spend: &mut Spend,
    publisher: &dyn Publisher,
    at: u64,
) -> Result<(), String> {
    let Some((mention, reply, author)) = memory
        .next_case_clarification()
        .map_err(|e| e.to_string())?
    else {
        return Ok(());
    };
    let mut journal =
        realorrug_journal::Journal::open(&paths.journal).map_err(|e| e.to_string())?;
    let reserved = spend
        .authorize(Cost::Reply, at / 86_400)
        .map_err(|e| e.to_string())?;
    spend.save().map_err(|e| e.to_string())?;
    memory
        .begin_case_clarification(&mention)
        .map_err(|e| e.to_string())?;
    let entry = crate::log::Entry {
        at,
        mention_id: mention,
        summoner: author,
        mint: None,
        read_at: None,
        read_at_slot: None,
        fact_sheet: "Identity unresolved; no chain read or verdict.".into(),
        reply,
        fellback: Some("clarification".into()),
        refused: None,
        reply_id: None,
        signals: None,
        pointed_at: None,
        level: None,
        leads: None,
    };
    let result = crate::publish::publish(publisher, &paths.log, &mut journal, entry);
    let charged = reserved.reserved();
    spend.settle(reserved, charged);
    spend.save().map_err(|e| e.to_string())?;
    result.map(|_| ()).map_err(|e| e.to_string())
}

fn valid_origin(origin: &str) -> bool {
    origin.strip_prefix("https://").is_some_and(|host| {
        let hostname = if let Some((hostname, port)) = host.rsplit_once(':') {
            if port.parse::<u16>().is_err() {
                return false;
            }
            hostname
        } else {
            host
        };
        !hostname.is_empty()
            && hostname.len() <= 253
            && hostname.split('.').all(|label| {
                !label.is_empty()
                    && label.len() <= 63
                    && label.starts_with(|c: char| c.is_ascii_alphanumeric())
                    && label.ends_with(|c: char| c.is_ascii_alphanumeric())
                    && label
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'-')
            })
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn public_origin_cannot_smuggle_a_path_credentials_or_invalid_port() {
        assert!(super::valid_origin("https://realorrug.example"));
        assert!(super::valid_origin("https://localhost:8443"));
        for origin in [
            "http://realorrug.example",
            "https://user:password@realorrug.example",
            "https://realorrug.example/path",
            "https://realorrug.example?token=secret",
            "https://realorrug.example:99999",
            "https://-bad.example",
            "https://realorrug..example",
        ] {
            assert!(!super::valid_origin(origin), "{origin}");
        }
    }
}
