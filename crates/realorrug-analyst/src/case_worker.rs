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

/// OS lock releases after a crash; overlapping workers must not share a meter.
///
/// # Errors
/// Another worker holds the file, or local storage is unavailable.
pub(crate) fn lease(
    paths: &Paths,
    get: &impl Fn(&str) -> Option<String>,
) -> Result<Option<std::fs::File>, String> {
    if capacity_from(get).is_none() {
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
pub fn initialize(paths: &Paths, get: &impl Fn(&str) -> Option<String>) -> Result<(), String> {
    if capacity_from(get).is_none() {
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
    get: &impl Fn(&str) -> Option<String>,
) -> Result<bool, String> {
    let Some(capacity) = capacity_from(get) else {
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
    get: &impl Fn(&str) -> Option<String>,
) -> Result<(), String> {
    if capacity_from(get).is_none() {
        return Ok(());
    }
    let memory = Memory::open(Path::new(&paths.memory)).map_err(|e| e.to_string())?;
    let at = crate::daemon::now();
    memory.case_heartbeat(at).map_err(|e| e.to_string())?;
    if let Some(job) = memory.next_case_job(at).map_err(|e| e.to_string())? {
        let base = get("REALORRUG_BASE_RPC")
            .filter(|v| !v.trim().is_empty())
            .map(realorrug_robinhood::Rpc::new);
        let ethereum = get("REALORRUG_ETHEREUM_RPC")
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
    deliver(&memory, paths, spend, publisher, at, get)
}

fn deliver(
    memory: &Memory,
    paths: &Paths,
    spend: &mut Spend,
    publisher: &dyn Publisher,
    at: u64,
    get: &impl Fn(&str) -> Option<String>,
) -> Result<(), String> {
    if get("REALORRUG_INVESTIGATOR_PUBLISH").as_deref() != Some("1")
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
    let origin = get("REALORRUG_PUBLIC_ORIGIN").ok_or("public dossier origin is missing")?;
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
    use super::*;
    use std::cell::RefCell;
    fn config(key: &str) -> Option<String> {
        Some(
            match key {
                "REALORRUG_ANALYST_GLOBAL_DAILY" => "10",
                "REALORRUG_ANALYST_PER_SUMMONER_DAILY" => "2",
                "REALORRUG_ANALYST_PER_CALL_USD" => "0.10",
                "REALORRUG_MONTHLY_USD" => "90",
                "REALORRUG_FIXED_MONTHLY_USD" => "60",
                "REALORRUG_INVESTIGATOR"
                | "REALORRUG_ANALYST_DAILY_USD"
                | "REALORRUG_X_PRICE_MENTION_READ"
                | "REALORRUG_X_PRICE_POST_READ"
                | "REALORRUG_X_PRICE_REPLY"
                | "REALORRUG_X_PRICE_POST"
                | "REALORRUG_X_PRICE_USER_READ"
                | "REALORRUG_MODEL_PER_CALL_USD_MICRO" => "1",
                _ => return None,
            }
            .into(),
        )
    }
    fn mention(id: &str, text: &str) -> Mention {
        Mention {
            id: id.into(),
            author: "user".into(),
            text: text.into(),
            parent: None,
            conversation: None,
        }
    }
    #[derive(Debug, Default)]
    struct Published(RefCell<Vec<String>>);
    impl Publisher for Published {
        fn name(&self) -> &'static str {
            "synthetic publisher"
        }
        fn reply(&self, _: &str, text: &str) -> Result<String, crate::publish::Undeliverable> {
            self.0.borrow_mut().push(text.into());
            Ok("synthetic-reply-id".into())
        }
        fn post(&self, _: &str) -> Result<String, crate::publish::Undeliverable> {
            panic!("case worker must never post to the timeline")
        }
    }
    #[test]
    fn worker_uses_only_nonblank_endpoints_for_the_requested_network() {
        for chain in [Network::Base, Network::Ethereum] {
            for endpoint in ["  ", "http://127.0.0.1:1"] {
                let dir = tempfile::tempdir().unwrap();
                let paths = Paths::under(dir.path().to_str().unwrap());
                let memory = Memory::open(Path::new(&paths.memory)).unwrap();
                let ready = mention(
                    "endpoint",
                    &format!("{chain} 0x1111111111111111111111111111111111111111 controls?"),
                );
                let mut gate = Gate::new(crate::daemon::limits_from(&config), vec![]);
                let at = crate::daemon::now();
                intake(&ready, &memory, &mut gate, at, &config).unwrap();
                let mut spend = Spend::open(
                    crate::daemon::budget_from(&config),
                    crate::spend::Prices::from_vars(&config).unwrap(),
                    paths.ledger.clone(),
                    at / 86_400,
                );
                let get = |key: &str| {
                    if key == "REALORRUG_BASE_RPC" || key == "REALORRUG_ETHEREUM_RPC" {
                        Some(endpoint.into())
                    } else {
                        config(key)
                    }
                };
                tick(
                    &paths,
                    &RpcClient::new("http://127.0.0.1:1"),
                    None,
                    None,
                    &mut spend,
                    &Published::default(),
                    &get,
                )
                .unwrap();
                let Resolved::Ready(request) = resolve(&ready, None) else {
                    panic!("ready")
                };
                let assessment = memory.case_assessment(&request).unwrap().unwrap();
                assert_eq!(assessment.observations.len(), 2);
                for observation in assessment.observations {
                    assert_eq!(
                        observation.gap.as_deref()
                            == Some("no endpoint configured for this network"),
                        endpoint.trim().is_empty()
                    );
                }
            }
        }
    }
    #[test]
    fn clarification_delivery_charges_the_current_day_once_and_records_its_receipt() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::under(dir.path().to_str().unwrap());
        let memory = Memory::open(Path::new(&paths.memory)).unwrap();
        let at = 86_400 * 100 + 99;
        memory
            .remember_case_clarification(
                "mention",
                "Please name the network",
                "actor",
                at,
                capacity_from(&config).unwrap(),
            )
            .unwrap();
        let mut spend = Spend::open(
            crate::daemon::budget_from(&config),
            crate::spend::Prices::from_vars(&config).unwrap(),
            paths.ledger.clone(),
            100,
        );
        let publisher = Published::default();
        deliver_clarification(&memory, &paths, &mut spend, &publisher, at).unwrap();
        deliver_clarification(&memory, &paths, &mut spend, &publisher, at + 1).unwrap();
        assert_eq!(publisher.0.borrow().as_slice(), ["Please name the network"]);
        let ledger: realorrug_provider::Ledger =
            serde_json::from_str(&std::fs::read_to_string(&paths.ledger).unwrap()).unwrap();
        assert_eq!(ledger.day, 100);
        assert_eq!(ledger.spent, 1);
        assert!(
            std::fs::read_to_string(&paths.log)
                .unwrap()
                .contains("synthetic-reply-id")
        );
    }
    #[test]
    fn worker_lease_and_recovery_refuse_overlap_or_a_corrupt_spend_ledger() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::under(dir.path().to_str().unwrap());
        assert!(lease(&paths, &|_| None).unwrap().is_none());
        let held = lease(&paths, &config).unwrap().unwrap();
        assert!(lease(&paths, &config).is_err());
        drop(held);
        let held = lease(&paths, &config).unwrap().unwrap();
        drop(held);
        std::fs::write(&paths.ledger, b"not a ledger").unwrap();
        assert!(lease(&paths, &config).unwrap_err().contains("invalid"));
        let memory = Memory::open(Path::new(&paths.memory)).unwrap();
        let Resolved::Ready(request) = resolve(
            &mention(
                "one",
                "base 0x1111111111111111111111111111111111111111 fees",
            ),
            None,
        ) else {
            panic!("valid request")
        };
        memory
            .enqueue_case(&request, "actor", 1, capacity_from(&config).unwrap())
            .unwrap();
        memory.next_case_job(2).unwrap();
        initialize(&paths, &|_| None).unwrap();
        assert_eq!(
            memory.case_job(&request.id).unwrap().unwrap().status,
            "running"
        );
        initialize(&paths, &config).unwrap();
        assert_eq!(
            memory.case_job(&request.id).unwrap().unwrap().status,
            "failed"
        );
        memory.verify_cases().unwrap();
    }
    #[test]
    fn mention_intake_preserves_claims_and_keeps_ambiguity_out_of_cases() {
        let dir = tempfile::tempdir().unwrap();
        let memory = Memory::open(&dir.path().join("cases.sqlite3")).unwrap();
        let mut gate = Gate::new(crate::daemon::limits_from(&config), vec!["self".into()]);
        let ready = mention(
            "one",
            "base 0x1111111111111111111111111111111111111111 where did fees go?",
        );
        assert!(!intake(&ready, &memory, &mut gate, 1, &|_| None).unwrap());
        assert!(
            memory
                .library_cases(None, "", u64::MAX, 10)
                .unwrap()
                .is_empty()
        );
        assert!(
            !intake(
                &mention("normal", "ordinary conversation"),
                &memory,
                &mut gate,
                1,
                &config
            )
            .unwrap()
        );
        assert!(
            !intake(
                &mention(
                    "legacy",
                    "robinhood 0x1111111111111111111111111111111111111111"
                ),
                &memory,
                &mut gate,
                1,
                &config
            )
            .unwrap()
        );
        let mut ignored = ready.clone();
        ignored.author = "self".into();
        assert!(intake(&ignored, &memory, &mut gate, 1, &config).unwrap());
        assert!(
            memory
                .library_cases(None, "", u64::MAX, 10)
                .unwrap()
                .is_empty()
        );
        assert!(intake(&ready, &memory, &mut gate, 2, &config).unwrap());
        assert!(intake(&ready, &memory, &mut gate, 3, &config).unwrap());
        assert_eq!(
            memory.library_cases(None, "", u64::MAX, 10).unwrap().len(),
            1
        );
        let job = memory.next_case_job(4).unwrap().unwrap();
        assert_eq!(job.request.question, ready.text);
        assert_eq!(job.request.case.chain, Network::Base);
        assert!(memory.next_case_delivery().unwrap().is_none());
        assert!(
            intake(
                &mention("ambiguous", "0x2222222222222222222222222222222222222222"),
                &memory,
                &mut gate,
                5,
                &config
            )
            .unwrap()
        );
        assert!(
            memory
                .next_case_clarification()
                .unwrap()
                .unwrap()
                .1
                .contains("network")
        );
        assert_eq!(
            memory.library_cases(None, "", u64::MAX, 10).unwrap().len(),
            1
        );
    }
    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "one durable result-to-publication lifecycle covers gates, recovery and statement selection"
    )]
    fn worker_results_precede_publication_and_each_delivery_is_attempted_once() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::under(dir.path().to_str().unwrap());
        let memory = Memory::open(Path::new(&paths.memory)).unwrap();
        let mut gate = Gate::new(crate::daemon::limits_from(&config), vec![]);
        let at = crate::daemon::now();
        let ready = mention(
            "one",
            "base 0x1111111111111111111111111111111111111111 fees",
        );
        intake(&ready, &memory, &mut gate, at, &config).unwrap();
        let Resolved::Ready(request) = resolve(&ready, None) else {
            panic!("valid request")
        };
        let mut spend = Spend::open(
            crate::daemon::budget_from(&config),
            crate::spend::Prices::from_vars(&config).unwrap(),
            paths.ledger.clone(),
            at / 86_400,
        );
        let publisher = Published::default();
        let rpc = RpcClient::new("http://127.0.0.1:1");
        tick(&paths, &rpc, None, None, &mut spend, &publisher, &|_| None).unwrap();
        assert_eq!(
            memory.case_job(&request.id).unwrap().unwrap().status,
            "pending"
        );
        tick(&paths, &rpc, None, None, &mut spend, &publisher, &config).unwrap();
        assert_eq!(
            memory.case_job(&request.id).unwrap().unwrap().status,
            "partial"
        );
        assert!(publisher.0.borrow().is_empty());
        let publish = |key: &str| match key {
            "REALORRUG_INVESTIGATOR_PUBLISH" => Some("1".into()),
            "REALORRUG_PUBLIC_ORIGIN" => Some("https://realorrug.example".into()),
            _ => config(key),
        };
        deliver(
            &memory,
            &paths,
            &mut spend,
            &crate::publish::DryRun,
            at,
            &publish,
        )
        .unwrap();
        assert!(memory.next_case_delivery().unwrap().is_some());
        assert!(
            deliver(&memory, &paths, &mut spend, &publisher, at, &|key| if key
                == "REALORRUG_PUBLIC_ORIGIN"
            {
                None
            } else {
                publish(key)
            })
            .unwrap_err()
            .contains("origin")
        );
        assert!(memory.next_case_delivery().unwrap().is_some());
        deliver(&memory, &paths, &mut spend, &publisher, at, &publish).unwrap();
        assert_eq!(publisher.0.borrow().len(), 1);
        let ledger: realorrug_provider::Ledger =
            serde_json::from_str(&std::fs::read_to_string(&paths.ledger).unwrap()).unwrap();
        assert_eq!(ledger.day, at / 86_400);
        assert_eq!(ledger.spent, 1);
        assert!(publisher.0.borrow()[0].contains("/library/base/"));
        assert!(publisher.0.borrow()[0].chars().count() <= 280);
        assert!(memory.next_case_delivery().unwrap().is_none());
        deliver(&memory, &paths, &mut spend, &publisher, at, &publish).unwrap();
        assert_eq!(publisher.0.borrow().len(), 1);
        let records = std::fs::read_to_string(&paths.log).unwrap();
        assert!(records.contains("synthetic-reply-id"));
        assert!(records.contains("\"mint\":null"));
        let second = mention(
            "two",
            "base 0x1111111111111111111111111111111111111111 follow-up fees",
        );
        intake(&second, &memory, &mut gate, at, &config).unwrap();
        let next = memory.next_case_job(at).unwrap().unwrap();
        let mut result = memory.case_assessment(&request).unwrap().unwrap();
        result.request_id = next.request.id.clone();
        result.findings = vec![
            realorrug_onchain::cases::Finding {
                kind: "synthetic".into(),
                text: "UNVERIFIED ALLEGATION".into(),
                evidence: vec![],
                counterevidence: vec![],
                status: "unresolved".into(),
            },
            realorrug_onchain::cases::Finding {
                kind: "synthetic".into(),
                text: "Long measured clause. ".repeat(30),
                evidence: vec![],
                counterevidence: vec![],
                status: "measured".into(),
            },
            realorrug_onchain::cases::Finding {
                kind: "synthetic".into(),
                text: "Reader measured a configured route.".into(),
                evidence: vec![],
                counterevidence: vec![],
                status: "measured".into(),
            },
        ];
        memory.finish_case(&next.request, &result, at).unwrap();
        deliver(&memory, &paths, &mut spend, &publisher, at, &publish).unwrap();
        let posted = publisher.0.borrow();
        assert_eq!(posted.len(), 2);
        assert!(posted[1].contains("Reader measured a configured route."));
        assert!(!posted[1].contains("UNVERIFIED"));
        assert!(!posted[1].contains("Long measured clause"));
        assert!(posted[1].chars().count() <= 280);
    }
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
