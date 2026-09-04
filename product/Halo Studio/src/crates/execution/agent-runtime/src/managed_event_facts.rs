//! Internal, Halo-owned contract for managed-task event facts.
//!
//! This module deliberately accepts only Halo-local identities, a closed fact
//! kind, and an already-redacted summary. External executor payloads, JSONL,
//! and delivery evidence stay outside this seam.

use std::fmt;
use std::sync::Arc;

use halo_runtime_ports::{
    ManagedEventFactAppend, ManagedEventFactKind as PortManagedEventFactKind,
    ManagedEventFactRecord, ManagedEventFactStorePort,
};

/// Version zero is a known legacy envelope. It remains readable so local
/// history can evolve additively, but Runtime writes always use the current
/// schema version.
pub(crate) const LEGACY_MANAGED_EVENT_FACT_SCHEMA_VERSION: u32 = 0;
pub(crate) const MANAGED_EVENT_FACT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ManagedEventFactKind {
    TaskLifecycle,
    UserMessageSummary,
    AgentReplySummary,
    ToolActivity,
    AgentOperationRequest,
    AgentOperationDecision,
    FileChangeFingerprint,
    TaskBaselineLinked,
    DeliveryEvidenceVersion,
    EvidenceFreshnessChanged,
}

/// A Runtime-owned summary which cannot be populated directly by an external
/// executor payload. Ticket 04 adds the production normalizer that constructs
/// this value after redaction and size limiting; this contract slice exposes
/// only a test helper until the Runtime is connected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ManagedEventFactSummary(String);

impl ManagedEventFactSummary {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    #[cfg(test)]
    fn test_redacted(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

const MAX_MANAGED_EVENT_SUMMARY_BYTES: usize = 512;

pub(crate) fn normalize_summary(value: &str) -> ManagedEventFactsResult<ManagedEventFactSummary> {
    let lower = value.to_ascii_lowercase();
    if value.contains('\0')
        || lower.contains("jsonl")
        || lower.contains("api_key")
        || lower.contains("private_key")
        || lower.contains("credential")
        || lower.contains("\"prompt")
        || lower.contains("\"response")
        || lower.contains("\"tool")
    {
        return Err(ManagedEventFactsError::UnsafePayload);
    }
    let mut normalized = String::new();
    for line in value.lines() {
        let lower = line.to_ascii_lowercase();
        if ["authorization", "cookie", "password", "secret", "token"]
            .iter()
            .any(|key| lower.contains(key))
        {
            normalized.push_str("[redacted]");
        } else {
            normalized.push_str(line);
        }
        normalized.push('\n');
        if normalized.len() >= MAX_MANAGED_EVENT_SUMMARY_BYTES {
            break;
        }
    }
    normalized.truncate(normalized.floor_char_boundary(MAX_MANAGED_EVENT_SUMMARY_BYTES));
    Ok(ManagedEventFactSummary(normalized.trim_end().to_string()))
}

#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd)]
pub(crate) struct HaloFactId(String);

#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd)]
pub(crate) struct HaloTaskId(String);

impl HaloFactId {
    pub(crate) fn from_runtime(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    #[cfg(test)]
    fn test(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

impl HaloTaskId {
    pub(crate) fn from_runtime(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    #[cfg(test)]
    fn test(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ManagedEventFactInput {
    pub fact_id: HaloFactId,
    pub task_id: HaloTaskId,
    pub recorded_at_ms: i64,
    pub schema_version: u32,
    pub kind: ManagedEventFactKind,
    pub redacted_summary: ManagedEventFactSummary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ManagedEventFact {
    pub fact_id: HaloFactId,
    pub task_id: HaloTaskId,
    pub sequence: u64,
    pub recorded_at_ms: i64,
    pub schema_version: u32,
    pub kind: ManagedEventFactKind,
    pub redacted_summary: ManagedEventFactSummary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ManagedEventFactsError {
    EmptyFactId,
    EmptyTaskId,
    FactIdentityConflict,
    InvalidRecordedSequence,
    UnsupportedSchema,
    UnsafePayload,
    Unavailable,
}

impl fmt::Display for ManagedEventFactsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyFactId => formatter.write_str("managed event fact identity is empty"),
            Self::EmptyTaskId => formatter.write_str("managed event fact task identity is empty"),
            Self::FactIdentityConflict => {
                formatter.write_str("managed event fact identity conflicts with recorded fact")
            }
            Self::InvalidRecordedSequence => {
                formatter.write_str("managed event fact recorded sequence is invalid")
            }
            Self::UnsupportedSchema => {
                formatter.write_str("managed event fact schema is unsupported")
            }
            Self::UnsafePayload => formatter.write_str("managed event fact payload is unsafe"),
            Self::Unavailable => formatter.write_str("managed event facts adapter is unavailable"),
        }
    }
}

impl std::error::Error for ManagedEventFactsError {}

pub(crate) type ManagedEventFactsResult<T> = Result<T, ManagedEventFactsError>;

/// Runtime-owned seam for the append-only, task-scoped fact history.
///
/// A provider assigns task-local sequence numbers and preserves the first fact
/// accepted for an identity. The Runtime normalizer is responsible for
/// redaction and size limits before it calls this interface.
pub(crate) trait ManagedEventFacts: Send + Sync {
    fn append(&self, input: ManagedEventFactInput) -> ManagedEventFactsResult<ManagedEventFact>;

    fn read_task(&self, task_id: &HaloTaskId) -> ManagedEventFactsResult<Vec<ManagedEventFact>>;
}

pub(crate) struct ManagedEventFactsPortAdapter {
    port: Arc<dyn ManagedEventFactStorePort>,
}

impl ManagedEventFactsPortAdapter {
    pub(crate) fn new(port: Arc<dyn ManagedEventFactStorePort>) -> Self {
        Self { port }
    }
}

impl ManagedEventFacts for ManagedEventFactsPortAdapter {
    fn append(&self, input: ManagedEventFactInput) -> ManagedEventFactsResult<ManagedEventFact> {
        validate_new_fact_input(&input)?;
        let record = self
            .port
            .append(ManagedEventFactAppend {
                task_id: input.task_id.0.clone(),
                fact_id: input.fact_id.0.clone(),
                recorded_at_ms: input.recorded_at_ms,
                schema_version: input.schema_version,
                kind: to_port_kind(input.kind),
                redacted_summary: input.redacted_summary.0.clone(),
            })
            .map_err(|_| ManagedEventFactsError::Unavailable)?;
        if record.task_id != input.task_id.0
            || record.fact_id != input.fact_id.0
            || record.schema_version != input.schema_version
            || record.sequence == 0
            || record.kind != to_port_kind(input.kind)
            || record.redacted_summary != input.redacted_summary.0
        {
            return Err(ManagedEventFactsError::FactIdentityConflict);
        }
        from_port_record(record)
    }

    fn read_task(&self, task_id: &HaloTaskId) -> ManagedEventFactsResult<Vec<ManagedEventFact>> {
        validate_task_id(&task_id.0)?;
        let records = self
            .port
            .read_task(&task_id.0)
            .map_err(|_| ManagedEventFactsError::Unavailable)?;
        let mut facts = Vec::with_capacity(records.len());
        for (index, record) in records.into_iter().enumerate() {
            if record.task_id != task_id.0 || record.sequence != index as u64 + 1 {
                return Err(ManagedEventFactsError::InvalidRecordedSequence);
            }
            facts.push(from_port_record(record)?);
        }
        Ok(facts)
    }
}

fn to_port_kind(kind: ManagedEventFactKind) -> PortManagedEventFactKind {
    match kind {
        ManagedEventFactKind::TaskLifecycle => PortManagedEventFactKind::TaskLifecycle,
        ManagedEventFactKind::UserMessageSummary => PortManagedEventFactKind::UserMessageSummary,
        ManagedEventFactKind::AgentReplySummary => PortManagedEventFactKind::AgentReplySummary,
        ManagedEventFactKind::ToolActivity => PortManagedEventFactKind::ToolActivity,
        ManagedEventFactKind::AgentOperationRequest => {
            PortManagedEventFactKind::AgentOperationRequest
        }
        ManagedEventFactKind::AgentOperationDecision => {
            PortManagedEventFactKind::AgentOperationDecision
        }
        ManagedEventFactKind::FileChangeFingerprint => {
            PortManagedEventFactKind::FileChangeFingerprint
        }
        ManagedEventFactKind::TaskBaselineLinked => PortManagedEventFactKind::TaskBaselineLinked,
        ManagedEventFactKind::DeliveryEvidenceVersion => {
            PortManagedEventFactKind::DeliveryEvidenceVersion
        }
        ManagedEventFactKind::EvidenceFreshnessChanged => {
            PortManagedEventFactKind::EvidenceFreshnessChanged
        }
    }
}

fn from_port_kind(kind: PortManagedEventFactKind) -> ManagedEventFactKind {
    match kind {
        PortManagedEventFactKind::TaskLifecycle => ManagedEventFactKind::TaskLifecycle,
        PortManagedEventFactKind::UserMessageSummary => ManagedEventFactKind::UserMessageSummary,
        PortManagedEventFactKind::AgentReplySummary => ManagedEventFactKind::AgentReplySummary,
        PortManagedEventFactKind::ToolActivity => ManagedEventFactKind::ToolActivity,
        PortManagedEventFactKind::AgentOperationRequest => {
            ManagedEventFactKind::AgentOperationRequest
        }
        PortManagedEventFactKind::AgentOperationDecision => {
            ManagedEventFactKind::AgentOperationDecision
        }
        PortManagedEventFactKind::FileChangeFingerprint => {
            ManagedEventFactKind::FileChangeFingerprint
        }
        PortManagedEventFactKind::TaskBaselineLinked => ManagedEventFactKind::TaskBaselineLinked,
        PortManagedEventFactKind::DeliveryEvidenceVersion => {
            ManagedEventFactKind::DeliveryEvidenceVersion
        }
        PortManagedEventFactKind::EvidenceFreshnessChanged => {
            ManagedEventFactKind::EvidenceFreshnessChanged
        }
    }
}

fn from_port_record(record: ManagedEventFactRecord) -> ManagedEventFactsResult<ManagedEventFact> {
    let summary = normalize_summary(&record.redacted_summary)?;
    let fact = ManagedEventFact {
        fact_id: HaloFactId::from_runtime(record.fact_id),
        task_id: HaloTaskId::from_runtime(record.task_id),
        sequence: record.sequence,
        recorded_at_ms: record.recorded_at_ms,
        schema_version: record.schema_version,
        kind: from_port_kind(record.kind),
        redacted_summary: summary,
    };
    validate_recorded_fact(&fact)?;
    Ok(fact)
}

#[derive(Default)]
pub(crate) struct InMemoryManagedEventFacts {
    facts_by_task: std::sync::Mutex<std::collections::BTreeMap<HaloTaskId, Vec<ManagedEventFact>>>,
}

#[cfg(test)]
impl InMemoryManagedEventFacts {
    /// Builds the test adapter from facts that were recorded before this
    /// process started. Production persistence remains outside this slice.
    fn from_recorded_facts(
        recorded_facts: impl IntoIterator<Item = ManagedEventFact>,
    ) -> ManagedEventFactsResult<Self> {
        let adapter = Self::default();
        let mut facts_by_task = adapter
            .facts_by_task
            .lock()
            .map_err(|_| ManagedEventFactsError::Unavailable)?;
        for fact in recorded_facts {
            validate_recorded_fact(&fact)?;
            let task_facts = facts_by_task.entry(fact.task_id.clone()).or_default();
            let expected_sequence = u64::try_from(task_facts.len())
                .ok()
                .and_then(|count| count.checked_add(1))
                .ok_or(ManagedEventFactsError::Unavailable)?;
            if fact.sequence != expected_sequence {
                return Err(ManagedEventFactsError::InvalidRecordedSequence);
            }
            if task_facts
                .iter()
                .any(|recorded| recorded.fact_id == fact.fact_id)
            {
                return Err(ManagedEventFactsError::FactIdentityConflict);
            }
            task_facts.push(fact);
        }
        drop(facts_by_task);
        Ok(adapter)
    }
}

impl ManagedEventFacts for InMemoryManagedEventFacts {
    fn append(&self, input: ManagedEventFactInput) -> ManagedEventFactsResult<ManagedEventFact> {
        validate_new_fact_input(&input)?;

        let mut facts_by_task = self
            .facts_by_task
            .lock()
            .map_err(|_| ManagedEventFactsError::Unavailable)?;
        let task_facts = facts_by_task.entry(input.task_id.clone()).or_default();
        if let Some(existing) = task_facts.iter().find(|fact| fact.fact_id == input.fact_id) {
            if existing.matches_input(&input) {
                return Ok(existing.clone());
            }
            return Err(ManagedEventFactsError::FactIdentityConflict);
        }
        let sequence = u64::try_from(task_facts.len())
            .ok()
            .and_then(|count| count.checked_add(1))
            .ok_or(ManagedEventFactsError::Unavailable)?;
        let fact = ManagedEventFact {
            fact_id: input.fact_id,
            task_id: input.task_id,
            sequence,
            recorded_at_ms: input.recorded_at_ms,
            schema_version: input.schema_version,
            kind: input.kind,
            redacted_summary: input.redacted_summary,
        };
        task_facts.push(fact.clone());
        Ok(fact)
    }

    fn read_task(&self, task_id: &HaloTaskId) -> ManagedEventFactsResult<Vec<ManagedEventFact>> {
        validate_task_id(&task_id.0)?;
        let facts_by_task = self
            .facts_by_task
            .lock()
            .map_err(|_| ManagedEventFactsError::Unavailable)?;
        Ok(facts_by_task.get(task_id).cloned().unwrap_or_default())
    }
}

fn validate_task_id(task_id: &str) -> ManagedEventFactsResult<()> {
    if task_id.trim().is_empty() {
        return Err(ManagedEventFactsError::EmptyTaskId);
    }
    Ok(())
}

fn validate_new_fact_input(input: &ManagedEventFactInput) -> ManagedEventFactsResult<()> {
    validate_task_id(&input.task_id.0)?;
    validate_fact_id(&input.fact_id.0)?;
    if input.schema_version != MANAGED_EVENT_FACT_SCHEMA_VERSION {
        return Err(ManagedEventFactsError::UnsupportedSchema);
    }
    Ok(())
}

fn validate_recorded_fact(fact: &ManagedEventFact) -> ManagedEventFactsResult<()> {
    validate_task_id(&fact.task_id.0)?;
    validate_fact_id(&fact.fact_id.0)?;
    if !is_readable_schema_version(fact.schema_version) {
        return Err(ManagedEventFactsError::UnsupportedSchema);
    }
    Ok(())
}

fn validate_fact_id(fact_id: &str) -> ManagedEventFactsResult<()> {
    if fact_id.trim().is_empty() {
        return Err(ManagedEventFactsError::EmptyFactId);
    }
    Ok(())
}

fn is_readable_schema_version(schema_version: u32) -> bool {
    matches!(
        schema_version,
        LEGACY_MANAGED_EVENT_FACT_SCHEMA_VERSION | MANAGED_EVENT_FACT_SCHEMA_VERSION
    )
}

impl ManagedEventFact {
    fn matches_input(&self, input: &ManagedEventFactInput) -> bool {
        self.fact_id == input.fact_id
            && self.task_id == input.task_id
            && self.recorded_at_ms == input.recorded_at_ms
            && self.schema_version == input.schema_version
            && self.kind == input.kind
            && self.redacted_summary == input.redacted_summary
    }
}

#[cfg(test)]
mod tests {
    use super::{
        HaloFactId, HaloTaskId, InMemoryManagedEventFacts, ManagedEventFact, ManagedEventFactInput,
        ManagedEventFactKind, ManagedEventFactSummary, ManagedEventFacts, ManagedEventFactsError,
        MANAGED_EVENT_FACT_SCHEMA_VERSION,
    };

    fn task_fact(
        fact_id: &str,
        recorded_at_ms: i64,
        kind: ManagedEventFactKind,
        redacted_summary: &str,
    ) -> ManagedEventFactInput {
        task_fact_for("task-1", fact_id, recorded_at_ms, kind, redacted_summary)
    }

    fn task_fact_for(
        task_id: &str,
        fact_id: &str,
        recorded_at_ms: i64,
        kind: ManagedEventFactKind,
        redacted_summary: &str,
    ) -> ManagedEventFactInput {
        ManagedEventFactInput {
            fact_id: HaloFactId::test(fact_id),
            task_id: HaloTaskId::test(task_id),
            recorded_at_ms,
            schema_version: MANAGED_EVENT_FACT_SCHEMA_VERSION,
            kind,
            redacted_summary: ManagedEventFactSummary::test_redacted(redacted_summary),
        }
    }

    #[test]
    fn normalizer_redacts_sensitive_lines_and_bounds_utf8_summary() {
        let value = format!("Authorization: bearer secret\n{}", "界".repeat(300));
        let summary = super::normalize_summary(&value).expect("safe summary boundary");

        assert!(summary.as_str().starts_with("[redacted]"));
        assert!(!summary.as_str().contains("bearer"));
        assert!(summary.as_str().len() <= super::MAX_MANAGED_EVENT_SUMMARY_BYTES);
        assert!(summary.as_str().is_char_boundary(summary.as_str().len()));
    }

    #[test]
    fn normalizer_rejects_raw_like_payloads() {
        for value in ["api_key=secret", "event jsonl payload", "\0 raw payload"] {
            assert_eq!(
                super::normalize_summary(value).expect_err("unsafe payload must fail closed"),
                ManagedEventFactsError::UnsafePayload
            );
        }
    }

    #[test]
    fn memory_adapter_reads_distinct_task_facts_in_append_order() {
        let adapter = InMemoryManagedEventFacts::default();
        let facts: &dyn ManagedEventFacts = &adapter;

        let created = facts
            .append(task_fact(
                "fact-1",
                100,
                ManagedEventFactKind::TaskLifecycle,
                "Managed task created.",
            ))
            .expect("append created fact");
        let requested = facts
            .append(task_fact(
                "fact-2",
                200,
                ManagedEventFactKind::AgentOperationRequest,
                "Developer decision requested.",
            ))
            .expect("append requested fact");

        assert_eq!(created.sequence, 1);
        assert_eq!(requested.sequence, 2);
        assert_eq!(
            facts
                .read_task(&HaloTaskId::test("task-1"))
                .expect("read task facts"),
            vec![created, requested]
        );
    }

    #[test]
    fn memory_adapter_reuses_the_first_fact_for_an_identical_identity() {
        let adapter = InMemoryManagedEventFacts::default();
        let facts: &dyn ManagedEventFacts = &adapter;
        let input = task_fact(
            "fact-1",
            100,
            ManagedEventFactKind::TaskLifecycle,
            "Managed task created.",
        );

        let first = facts.append(input.clone()).expect("append first fact");
        let duplicate = facts.append(input).expect("append duplicate fact");

        assert_eq!(duplicate, first);
        assert_eq!(
            facts
                .read_task(&HaloTaskId::test("task-1"))
                .expect("read task facts"),
            vec![first]
        );
    }

    #[test]
    fn memory_adapter_rejects_conflicting_reuse_of_a_fact_identity() {
        let adapter = InMemoryManagedEventFacts::default();
        let facts: &dyn ManagedEventFacts = &adapter;
        let first = facts
            .append(task_fact(
                "fact-1",
                100,
                ManagedEventFactKind::TaskLifecycle,
                "Managed task created.",
            ))
            .expect("append first fact");

        let error = facts
            .append(task_fact(
                "fact-1",
                200,
                ManagedEventFactKind::TaskLifecycle,
                "Managed task started.",
            ))
            .expect_err("conflicting fact identity must fail");

        assert_eq!(
            error.to_string(),
            "managed event fact identity conflicts with recorded fact"
        );
        assert_eq!(
            facts
                .read_task(&HaloTaskId::test("task-1"))
                .expect("read task facts"),
            vec![first]
        );
    }

    #[test]
    fn memory_adapter_rejects_an_unsupported_schema_without_appending() {
        let adapter = InMemoryManagedEventFacts::default();
        let facts: &dyn ManagedEventFacts = &adapter;
        let mut input = task_fact(
            "fact-1",
            100,
            ManagedEventFactKind::TaskLifecycle,
            "Managed task created.",
        );
        input.schema_version = MANAGED_EVENT_FACT_SCHEMA_VERSION + 1;

        let error = facts
            .append(input)
            .expect_err("unsupported schema must fail closed");

        assert_eq!(
            error.to_string(),
            "managed event fact schema is unsupported"
        );
        assert!(facts
            .read_task(&HaloTaskId::test("task-1"))
            .expect("read task facts")
            .is_empty());
    }

    #[test]
    fn memory_adapter_reads_a_known_legacy_schema_fact() {
        let legacy_fact = ManagedEventFact {
            fact_id: HaloFactId::test("fact-1"),
            task_id: HaloTaskId::test("task-1"),
            sequence: 1,
            recorded_at_ms: 100,
            schema_version: 0,
            kind: ManagedEventFactKind::TaskLifecycle,
            redacted_summary: ManagedEventFactSummary::test_redacted("Managed task created."),
        };
        let adapter = InMemoryManagedEventFacts::from_recorded_facts([legacy_fact.clone()])
            .expect("load a known legacy fact");
        let facts: &dyn ManagedEventFacts = &adapter;

        assert_eq!(
            facts
                .read_task(&HaloTaskId::test("task-1"))
                .expect("read task facts"),
            vec![legacy_fact]
        );
    }

    #[test]
    fn memory_adapter_rejects_recorded_facts_with_nonsequential_sequences() {
        let first = ManagedEventFact {
            fact_id: HaloFactId::test("fact-1"),
            task_id: HaloTaskId::test("task-1"),
            sequence: 1,
            recorded_at_ms: 100,
            schema_version: MANAGED_EVENT_FACT_SCHEMA_VERSION,
            kind: ManagedEventFactKind::TaskLifecycle,
            redacted_summary: ManagedEventFactSummary::test_redacted("Managed task created."),
        };
        let skipped = ManagedEventFact {
            fact_id: HaloFactId::test("fact-2"),
            task_id: HaloTaskId::test("task-1"),
            sequence: 3,
            recorded_at_ms: 200,
            schema_version: MANAGED_EVENT_FACT_SCHEMA_VERSION,
            kind: ManagedEventFactKind::TaskLifecycle,
            redacted_summary: ManagedEventFactSummary::test_redacted("Managed task started."),
        };

        assert_eq!(
            InMemoryManagedEventFacts::from_recorded_facts([first, skipped]).err(),
            Some(ManagedEventFactsError::InvalidRecordedSequence)
        );
    }

    #[test]
    fn memory_adapter_rejects_recorded_facts_with_duplicate_identities() {
        let first = ManagedEventFact {
            fact_id: HaloFactId::test("fact-1"),
            task_id: HaloTaskId::test("task-1"),
            sequence: 1,
            recorded_at_ms: 100,
            schema_version: MANAGED_EVENT_FACT_SCHEMA_VERSION,
            kind: ManagedEventFactKind::TaskLifecycle,
            redacted_summary: ManagedEventFactSummary::test_redacted("Managed task created."),
        };
        let duplicate = ManagedEventFact {
            fact_id: HaloFactId::test("fact-1"),
            task_id: HaloTaskId::test("task-1"),
            sequence: 2,
            recorded_at_ms: 200,
            schema_version: MANAGED_EVENT_FACT_SCHEMA_VERSION,
            kind: ManagedEventFactKind::TaskLifecycle,
            redacted_summary: ManagedEventFactSummary::test_redacted("Managed task started."),
        };

        assert_eq!(
            InMemoryManagedEventFacts::from_recorded_facts([first, duplicate]).err(),
            Some(ManagedEventFactsError::FactIdentityConflict)
        );
    }

    #[test]
    fn memory_adapter_keeps_distinct_identities_with_the_same_summary() {
        let adapter = InMemoryManagedEventFacts::default();
        let facts: &dyn ManagedEventFacts = &adapter;

        let first = facts
            .append(task_fact(
                "fact-1",
                100,
                ManagedEventFactKind::TaskLifecycle,
                "Managed task state changed.",
            ))
            .expect("append first fact");
        let second = facts
            .append(task_fact(
                "fact-2",
                200,
                ManagedEventFactKind::TaskLifecycle,
                "Managed task state changed.",
            ))
            .expect("append second fact");

        assert_eq!(
            facts
                .read_task(&HaloTaskId::test("task-1"))
                .expect("read task facts"),
            vec![first, second]
        );
    }

    #[test]
    fn memory_adapter_rejects_a_blank_task_identity_without_appending() {
        let adapter = InMemoryManagedEventFacts::default();
        let facts: &dyn ManagedEventFacts = &adapter;
        let mut input = task_fact(
            "fact-1",
            100,
            ManagedEventFactKind::TaskLifecycle,
            "Managed task created.",
        );
        input.task_id = HaloTaskId::test("  ");

        let error = facts
            .append(input)
            .expect_err("blank task identity must fail");

        assert_eq!(
            error.to_string(),
            "managed event fact task identity is empty"
        );
        assert!(facts
            .read_task(&HaloTaskId::test("task-1"))
            .expect("read task facts")
            .is_empty());
    }

    #[test]
    fn memory_adapter_rejects_a_blank_fact_identity_without_appending() {
        let adapter = InMemoryManagedEventFacts::default();
        let facts: &dyn ManagedEventFacts = &adapter;
        let mut input = task_fact(
            "fact-1",
            100,
            ManagedEventFactKind::TaskLifecycle,
            "Managed task created.",
        );
        input.fact_id = HaloFactId::test("  ");

        let error = facts
            .append(input)
            .expect_err("blank fact identity must fail");

        assert_eq!(error.to_string(), "managed event fact identity is empty");
        assert!(facts
            .read_task(&HaloTaskId::test("task-1"))
            .expect("read task facts")
            .is_empty());
    }

    #[test]
    fn memory_adapter_scopes_identity_and_sequence_to_each_task() {
        let adapter = InMemoryManagedEventFacts::default();
        let facts: &dyn ManagedEventFacts = &adapter;

        let first_task_fact = facts
            .append(task_fact_for(
                "task-1",
                "fact-1",
                100,
                ManagedEventFactKind::TaskLifecycle,
                "First managed task created.",
            ))
            .expect("append first task fact");
        let second_task_fact = facts
            .append(task_fact_for(
                "task-2",
                "fact-1",
                200,
                ManagedEventFactKind::TaskLifecycle,
                "Second managed task created.",
            ))
            .expect("append second task fact");

        assert_eq!(first_task_fact.sequence, 1);
        assert_eq!(second_task_fact.sequence, 1);
        assert_eq!(
            facts
                .read_task(&HaloTaskId::test("task-1"))
                .expect("read first task facts"),
            vec![first_task_fact]
        );
        assert_eq!(
            facts
                .read_task(&HaloTaskId::test("task-2"))
                .expect("read second task facts"),
            vec![second_task_fact]
        );
    }

    #[test]
    fn memory_adapter_returns_a_copy_of_recorded_task_facts() {
        let adapter = InMemoryManagedEventFacts::default();
        let facts: &dyn ManagedEventFacts = &adapter;
        facts
            .append(task_fact(
                "fact-1",
                100,
                ManagedEventFactKind::TaskLifecycle,
                "Managed task created.",
            ))
            .expect("append task fact");

        let mut read = facts
            .read_task(&HaloTaskId::test("task-1"))
            .expect("read task facts");
        read[0].redacted_summary = ManagedEventFactSummary::test_redacted("Locally modified copy.");

        assert_eq!(
            facts
                .read_task(&HaloTaskId::test("task-1"))
                .expect("reread task facts")[0]
                .redacted_summary
                .as_str(),
            "Managed task created."
        );
    }

    #[test]
    fn memory_adapter_rejects_a_blank_task_identity_when_reading() {
        let adapter = InMemoryManagedEventFacts::default();
        let facts: &dyn ManagedEventFacts = &adapter;

        let error = facts
            .read_task(&HaloTaskId::test("  "))
            .expect_err("blank task identity must not read as an empty history");

        assert_eq!(
            error.to_string(),
            "managed event fact task identity is empty"
        );
    }
}
