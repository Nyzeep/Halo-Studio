use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use fs2::FileExt;
use halo_runtime_ports::{
    ManagedEventFactAppend, ManagedEventFactRecord, ManagedEventFactStorePort, PortError,
    PortErrorKind, PortResult,
};
use serde::{Deserialize, Serialize};

const SCHEMA_VERSION: u32 = 1;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManagedEventFactsFile {
    schema_version: u32,
    facts: Vec<ManagedEventFactRecord>,
}

pub struct JsonFileManagedEventFacts {
    path: PathBuf,
}

impl JsonFileManagedEventFacts {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn lock_path(&self) -> PathBuf {
        self.path.with_extension("json.lock")
    }

    fn with_lock<T>(&self, operation: impl FnOnce() -> PortResult<T>) -> PortResult<T> {
        let parent = self.path.parent().ok_or_else(|| {
            PortError::new(PortErrorKind::Backend, "managed facts path has no parent")
        })?;
        fs::create_dir_all(parent).map_err(|_| {
            PortError::new(
                PortErrorKind::Backend,
                "managed facts directory could not be prepared",
            )
        })?;
        let lock = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(self.lock_path())
            .map_err(|_| {
                PortError::new(PortErrorKind::Backend, "managed facts lock unavailable")
            })?;
        lock.lock_exclusive().map_err(|_| {
            PortError::new(PortErrorKind::Backend, "managed facts lock unavailable")
        })?;
        let result = operation();
        let _ = lock.unlock();
        result
    }

    fn load_unlocked(&self) -> PortResult<Vec<ManagedEventFactRecord>> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(_) => {
                return Err(PortError::new(
                    PortErrorKind::Backend,
                    "managed facts could not be read",
                ))
            }
        };
        let file: ManagedEventFactsFile = serde_json::from_slice(&bytes).map_err(|_| {
            PortError::new(PortErrorKind::InvalidRequest, "managed facts are invalid")
        })?;
        if file.schema_version != 0 && file.schema_version != SCHEMA_VERSION {
            return Err(PortError::new(
                PortErrorKind::InvalidRequest,
                "managed facts schema is unsupported",
            ));
        }
        Ok(file.facts)
    }

    fn save_unlocked(&self, facts: &[ManagedEventFactRecord]) -> PortResult<()> {
        let file = ManagedEventFactsFile {
            schema_version: SCHEMA_VERSION,
            facts: facts.to_vec(),
        };
        let bytes = serde_json::to_vec(&file).map_err(|_| {
            PortError::new(PortErrorKind::Backend, "managed facts could not be encoded")
        })?;
        let temporary = self.path.with_extension(format!(
            "json.{}.tmp",
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let mut output = File::create(&temporary).map_err(|_| {
            PortError::new(
                PortErrorKind::Backend,
                "managed facts temporary file unavailable",
            )
        })?;
        output.write_all(&bytes).map_err(|_| {
            PortError::new(PortErrorKind::Backend, "managed facts could not be written")
        })?;
        output.sync_all().map_err(|_| {
            PortError::new(PortErrorKind::Backend, "managed facts could not be flushed")
        })?;
        drop(output);
        fs::rename(&temporary, &self.path).map_err(|_| {
            let _ = fs::remove_file(&temporary);
            PortError::new(
                PortErrorKind::Backend,
                "managed facts could not be committed",
            )
        })
    }
}

impl ManagedEventFactStorePort for JsonFileManagedEventFacts {
    fn append(&self, fact: ManagedEventFactAppend) -> PortResult<ManagedEventFactRecord> {
        self.with_lock(|| {
            let mut facts = self.load_unlocked()?;
            if let Some(existing) = facts
                .iter()
                .find(|record| record.task_id == fact.task_id && record.fact_id == fact.fact_id)
            {
                return Ok(existing.clone());
            }
            let sequence = facts
                .iter()
                .filter(|record| record.task_id == fact.task_id)
                .count() as u64
                + 1;
            let record = ManagedEventFactRecord {
                task_id: fact.task_id,
                fact_id: fact.fact_id,
                sequence,
                recorded_at_ms: fact.recorded_at_ms,
                schema_version: fact.schema_version,
                kind: fact.kind,
                redacted_summary: fact.redacted_summary,
            };
            facts.push(record.clone());
            self.save_unlocked(&facts)?;
            Ok(record)
        })
    }

    fn read_task(&self, task_id: &str) -> PortResult<Vec<ManagedEventFactRecord>> {
        self.with_lock(|| {
            Ok(self
                .load_unlocked()?
                .into_iter()
                .filter(|record| record.task_id == task_id)
                .collect())
        })
    }
}
