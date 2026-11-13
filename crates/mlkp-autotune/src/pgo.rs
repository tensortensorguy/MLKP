// CEP:FILE: crates/mlkp-autotune/src/pgo.rs
// CEP:WHAT: The PGO store: persistent shape distributions, execution frequencies, and measured costs keyed by subgraph hash, with drift detection that triggers re-autotuning.
// CEP:WHY: Design.md makes PGO the nervous system of the compiler; every execution of optimized code flows new measurements back through this store, and the drift detector implements the design's PGO-drift failure mode.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Load and save return PgoError on unreadable or malformed cache files; detection never panics.
// CEP:ASSUMES: the cache directory is writable; concurrent access is serialized by the driver.
// CEP:COST: cold; persistence runs at compile boundaries, recording runs O(observations).
// CEP:EVIDENCE: unit tests in crates/mlkp-autotune/tests/pgo_test.rs.

#![deny(missing_docs)]

use std::path::{Path, PathBuf};

use cep_json::Json;

/// PGO store failure.
#[derive(Clone, Debug, PartialEq)]
pub enum PgoError {
    /// The cache file could not be read.
    Io(String),
    /// The cache file was not valid JSON or had the wrong shape.
    Malformed(String),
}

impl std::fmt::Display for PgoError {
    // CEP:WHAT: Formats the PGO error.
    // CEP:WHY: Deterministic diagnostics.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: cold; one formatted string.
    // CEP:EVIDENCE: crates/mlkp-autotune/tests/pgo_test.rs
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PgoError::Io(s) => write!(f, "pgo error: {s}"),
            PgoError::Malformed(s) => write!(f, "pgo error: {s}"),
        }
    }
}

impl std::error::Error for PgoError {}

/// One recorded observation for a subgraph.
#[derive(Clone, Debug, PartialEq)]
pub struct Observation {
    /// Concrete dims of the run.
    pub dims: Vec<u64>,
    /// Execution count contribution.
    pub exec_count: u64,
    /// Measured cost in nanoseconds.
    pub cost_ns: u64,
}

/// The stored winning schedule for warm starting.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StoredConfig {
    /// Tile rows.
    pub bm: u32,
    /// Tile columns.
    pub bn: u32,
    /// Tile depth.
    pub bk: u32,
    /// Worker count.
    pub threads: u32,
    /// Loop order.
    pub loop_order: u32,
    /// Transposed B layout.
    pub transpose_b: bool,
    /// Pass selection bits: (algebraic, cse, dce).
    pub passes: (bool, bool, bool),
    /// Algebraic fixed-point sweeps.
    pub algebraic_iters: u32,
}

/// PGO record for one subgraph.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PgoRecord {
    /// Total observed executions.
    pub exec_count: u64,
    /// Shape observations with counts.
    pub shapes: Vec<(Vec<u64>, u64)>,
    /// Mean measured cost across recent observations, nanoseconds.
    pub mean_cost_ns: u64,
    /// Most recent best plan hash for warm starting.
    pub last_best_config: u64,
    /// Observation count feeding the mean.
    pub cost_samples: u64,
    /// The winning schedule fields, when a compilation completed.
    pub last_best: Option<StoredConfig>,
}

/// The persistent PGO store.
#[derive(Clone, Debug)]
pub struct PgoStore {
    records: Vec<(u64, PgoRecord)>,
    path: Option<PathBuf>,
}

impl PgoStore {
    /// Creates an empty in-memory store.
    // CEP:WHAT: Creates an empty store.
    // CEP:WHY: Tests and cold starts without a cache directory.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-autotune/tests/pgo_test.rs
    pub fn in_memory() -> PgoStore {
        PgoStore {
            records: Vec::new(),
            path: None,
        }
    }

    /// Attaches a persistence path.
    // CEP:WHAT: Attaches the cache path.
    // CEP:WHY: The design's PGO cache survives process restarts.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-autotune/tests/pgo_test.rs
    pub fn with_path(mut self, path: &Path) -> PgoStore {
        self.path = Some(path.to_path_buf());
        self
    }

    /// Loads records from the attached path; a missing file yields an empty store.
    // CEP:WHAT: Loads persisted records.
    // CEP:WHY: Warm-started cost models on the first compile of a new process (design PGO cache).
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns PgoError::Io for unreadable files and Malformed for wrong shapes; a missing file is not an error.
    // CEP:ASSUMES: the file was written by save().
    // CEP:COST: O(file size).
    // CEP:EVIDENCE: crates/mlkp-autotune/tests/pgo_test.rs
    pub fn load(&mut self) -> Result<(), PgoError> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(PgoError::Io(e.to_string())),
        };
        let value = cep_json::parse(&text).map_err(|e| PgoError::Malformed(e.to_string()))?;
        let arr = value
            .get("records")
            .and_then(|v| v.as_arr())
            .ok_or_else(|| PgoError::Malformed("records array missing".to_string()))?;
        let mut out = Vec::new();
        for item in arr {
            // CEP:WHAT: Parses 64-bit keys from their decimal string form.
            // CEP:WHY: JSON numbers are IEEE doubles; a u64 hash above 2^53 would round and corrupt the key, so keys travel as strings both ways.
            // CEP:STATUS: complete
            // CEP:FAILURE: Returns Malformed when the hash string is missing or unparseable.
            // CEP:ASSUMES: the file was written by save().
            // CEP:COST: O(digits).
            // CEP:EVIDENCE: crates/mlkp-autotune/tests/pgo_test.rs
            let hash = item
                .get("hash")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<u64>().ok())
                .ok_or_else(|| PgoError::Malformed("record hash missing".to_string()))?;
            let rec = item
                .get("record")
                .ok_or_else(|| PgoError::Malformed("record missing".to_string()))?;
            let exec_count = rec.get("exec_count").and_then(|v| v.as_u64()).unwrap_or(0);
            let mean_cost_ns = rec
                .get("mean_cost_ns")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let last = rec
                .get("last_best_config")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
            let cost_samples = rec
                .get("cost_samples")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let last_best = parse_stored_config(rec);
            let mut shapes = Vec::new();
            if let Some(shape_arr) = rec.get("shapes").and_then(|v| v.as_arr()) {
                for s in shape_arr {
                    let dims: Vec<u64> = s
                        .get("dims")
                        .and_then(|v| v.as_arr())
                        .map(|a| a.iter().filter_map(|x| x.as_u64()).collect())
                        .unwrap_or_default();
                    let count = s.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
                    shapes.push((dims, count));
                }
            }
            out.push((
                hash,
                PgoRecord {
                    exec_count,
                    shapes,
                    mean_cost_ns,
                    last_best_config: last,
                    cost_samples,
                    last_best,
                },
            ));
        }
        self.records = out;
        Ok(())
    }

    /// Saves all records to the attached path.
    // CEP:WHAT: Persists all records.
    // CEP:WHY: The PGO cache outlives the process.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns PgoError::Io when the directory or file cannot be written.
    // CEP:ASSUMES: none
    // CEP:COST: O(records).
    // CEP:EVIDENCE: crates/mlkp-autotune/tests/pgo_test.rs
    pub fn save(&self) -> Result<(), PgoError> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| PgoError::Io(e.to_string()))?;
        }
        let mut records = Vec::new();
        for (hash, rec) in &self.records {
            let mut shapes = Vec::new();
            for (dims, count) in &rec.shapes {
                shapes.push(Json::Obj(vec![
                    (
                        "dims".to_string(),
                        Json::Arr(dims.iter().map(|d| Json::Num(*d as f64)).collect()),
                    ),
                    ("count".to_string(), Json::Num(*count as f64)),
                ]));
            }
            let mut fields = vec![
                ("exec_count".to_string(), Json::Num(rec.exec_count as f64)),
                ("shapes".to_string(), Json::Arr(shapes)),
                (
                    "mean_cost_ns".to_string(),
                    Json::Num(rec.mean_cost_ns as f64),
                ),
                (
                    "last_best_config".to_string(),
                    Json::Str(rec.last_best_config.to_string()),
                ),
                (
                    "cost_samples".to_string(),
                    Json::Num(rec.cost_samples as f64),
                ),
            ];
            if let Some(cfg) = rec.last_best {
                fields.push(("last_best".to_string(), stored_config_json(cfg)));
            }
            records.push(Json::Obj(vec![
                // CEP:WHAT: Serializes the 64-bit key as a decimal string.
                // CEP:WHY: A u64 above 2^53 does not round-trip through an IEEE double; the string form is exact.
                // CEP:STATUS: complete
                // CEP:FAILURE: none
                // CEP:ASSUMES: none
                // CEP:COST: O(digits).
                // CEP:EVIDENCE: crates/mlkp-autotune/tests/pgo_test.rs
                ("hash".to_string(), Json::Str(hash.to_string())),
                ("record".to_string(), Json::Obj(fields)),
            ]));
        }
        let doc = Json::Obj(vec![("records".to_string(), Json::Arr(records))]);
        let text = cep_json::write::to_string_pretty(&doc);
        std::fs::write(path, text).map_err(|e| PgoError::Io(e.to_string()))?;
        Ok(())
    }

    /// Records one execution observation for a subgraph.
    // CEP:WHAT: Records one observation.
    // CEP:WHY: The feedback loop of design stage 6: every execution makes the next compilation better.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(shapes).
    // CEP:EVIDENCE: crates/mlkp-autotune/tests/pgo_test.rs
    pub fn record(&mut self, subgraph_hash: u64, obs: &Observation) {
        let rec = self
            .records
            .iter_mut()
            .find(|(h, _)| *h == subgraph_hash)
            .map(|(_, r)| r);
        let rec = match rec {
            Some(r) => r,
            None => {
                self.records.push((subgraph_hash, PgoRecord::default()));
                &mut self.records.last_mut().expect("just pushed").1
            }
        };
        rec.exec_count = rec.exec_count.saturating_add(obs.exec_count);
        rec.cost_samples = rec.cost_samples.saturating_add(1);
        // CEP:WHAT: Updates the running mean in 128-bit arithmetic.
        // CEP:WHY: The product mean * (n-1) can exceed 64 bits at large sample counts; u128 keeps the accumulator exact and the division truncation is documented.
        // CEP:STATUS: complete
        // CEP:FAILURE: none
        // CEP:ASSUMES: none
        // CEP:COST: O(1).
        // CEP:EVIDENCE: crates/mlkp-autotune/tests/pgo_test.rs
        let n = rec.cost_samples.max(1) as u128;
        let acc = rec.mean_cost_ns as u128 * (n - 1) + obs.cost_ns as u128;
        rec.mean_cost_ns = (acc / n).min(u64::MAX as u128) as u64;
        if let Some(entry) = rec.shapes.iter_mut().find(|(d, _)| *d == obs.dims) {
            entry.1 = entry.1.saturating_add(obs.exec_count);
        } else {
            rec.shapes.push((obs.dims.clone(), obs.exec_count));
        }
    }

    /// Reads one record.
    // CEP:WHAT: Reads one record.
    // CEP:WHY: Hot-path triggers and warm starts read frequencies.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns None for unknown subgraphs.
    // CEP:ASSUMES: none
    // CEP:COST: O(records).
    // CEP:EVIDENCE: crates/mlkp-autotune/tests/pgo_test.rs
    pub fn record_for(&self, subgraph_hash: u64) -> Option<&PgoRecord> {
        self.records
            .iter()
            .find(|(h, _)| *h == subgraph_hash)
            .map(|(_, r)| r)
    }

    /// Sets the last best config for warm starting.
    // CEP:WHAT: Stores the last best config hash.
    // CEP:WHY: Incremental recompilation warm starts from the previous winner.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(records).
    // CEP:EVIDENCE: crates/mlkp-autotune/tests/pgo_test.rs
    pub fn set_last_best(&mut self, subgraph_hash: u64, config_hash: u64) {
        if let Some((_, rec)) = self.records.iter_mut().find(|(h, _)| *h == subgraph_hash) {
            rec.last_best_config = config_hash;
        }
    }

    /// Sets the last best config and its concrete schedule fields for warm starting.
    // CEP:WHAT: Stores the winner hash and schedule together.
    // CEP:WHY: A hash alone cannot reconstruct the schedule; warm starting must reinstall the previous winner's tile and loop settings.
    // CEP:STATUS: complete
    // CEP:FAILURE: Creates the record when absent so the warm start survives process restarts.
    // CEP:ASSUMES: none
    // CEP:COST: O(records).
    // CEP:EVIDENCE: crates/mlkp-autotune/tests/pgo_test.rs
    pub fn set_last_best_config(
        &mut self,
        subgraph_hash: u64,
        config_hash: u64,
        config: StoredConfig,
    ) {
        if self.record_for(subgraph_hash).is_none() {
            self.records.push((subgraph_hash, PgoRecord::default()));
        }
        self.set_last_best(subgraph_hash, config_hash);
        if let Some((_, rec)) = self.records.iter_mut().find(|(h, _)| *h == subgraph_hash) {
            rec.last_best = Some(config);
        }
    }

    /// Detects drift: true when the fresh median cost exceeds the stored mean by the drift ratio.
    // CEP:WHAT: Detects PGO drift.
    // CEP:WHY: Design failure mode "PGO drift": when runtime behavior diverges beyond a threshold, trigger re-autotune with fresh data.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns false when no record exists.
    // CEP:ASSUMES: fresh_ns is a median of the current measurement series.
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-autotune/tests/pgo_test.rs
    pub fn drift_detected(&self, subgraph_hash: u64, fresh_ns: u64) -> bool {
        let ratio = mlkp_config::limits::PGO_DRIFT_RATIO;
        self.record_for(subgraph_hash).is_some_and(|r| {
            r.cost_samples >= 4 && (fresh_ns as f64) > r.mean_cost_ns as f64 * ratio
        })
    }
}

// CEP:WHAT: Serializes one stored schedule to JSON.
// CEP:WHY: Warm starting needs the previous winner's concrete fields, not just its hash.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-autotune/tests/pgo_test.rs
fn stored_config_json(cfg: StoredConfig) -> Json {
    Json::Obj(vec![
        ("bm".to_string(), Json::Num(f64::from(cfg.bm))),
        ("bn".to_string(), Json::Num(f64::from(cfg.bn))),
        ("bk".to_string(), Json::Num(f64::from(cfg.bk))),
        ("threads".to_string(), Json::Num(f64::from(cfg.threads))),
        (
            "loop_order".to_string(),
            Json::Num(f64::from(cfg.loop_order)),
        ),
        ("transpose_b".to_string(), Json::Bool(cfg.transpose_b)),
        (
            "passes".to_string(),
            Json::Arr(vec![
                Json::Bool(cfg.passes.0),
                Json::Bool(cfg.passes.1),
                Json::Bool(cfg.passes.2),
            ]),
        ),
        (
            "algebraic_iters".to_string(),
            Json::Num(f64::from(cfg.algebraic_iters)),
        ),
    ])
}

// CEP:WHAT: Parses one stored schedule from a record; absent fields yield None.
// CEP:WHY: Older cache files without the schedule must still load; warm start then falls back to the default schedule.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-autotune/tests/pgo_test.rs
fn parse_stored_config(rec: &Json) -> Option<StoredConfig> {
    let obj = rec.get("last_best")?;
    let num = |k: &str| obj.get(k).and_then(|v| v.as_u64());
    let bm = num("bm")?;
    let bn = num("bn")?;
    let bk = num("bk")?;
    let threads = num("threads")?;
    let loop_order = num("loop_order")?;
    let transpose_b = obj
        .get("transpose_b")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let passes = obj.get("passes").and_then(|v| v.as_arr()).map(|arr| {
        let bit = |i: usize| arr.get(i).and_then(|v| v.as_bool()).unwrap_or(true);
        (bit(0), bit(1), bit(2))
    });
    let algebraic_iters = num("algebraic_iters").unwrap_or(1);
    Some(StoredConfig {
        bm: bm.min(u32::MAX as u64) as u32,
        bn: bn.min(u32::MAX as u64) as u32,
        bk: bk.min(u32::MAX as u64) as u32,
        threads: threads.min(u32::MAX as u64) as u32,
        loop_order: loop_order.min(u32::MAX as u64) as u32,
        transpose_b,
        passes: passes.unwrap_or((true, true, true)),
        algebraic_iters: algebraic_iters.min(u32::MAX as u64) as u32,
    })
}
