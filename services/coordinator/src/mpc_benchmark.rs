//! MPC performance benchmarking for deal phase.
//!
//! Tracks latency breakdown by operation phase:
//! - Secret sharing: time to distribute shares across nodes
//! - Shuffling: time for deck shuffling via MPC
//! - Commitment: time to compute hand commitments
//! - Proving: time to generate ZK proofs
//!
//! Supports testing with 2-6 players.

use std::collections::HashMap;
use std::time::Instant;

#[derive(Clone, Debug, serde::Serialize)]
pub struct PhaseTiming {
    pub phase: String,
    pub duration_ms: u64,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct DealBenchmark {
    pub table_id: u32,
    pub player_count: usize,
    pub total_duration_ms: u64,
    pub phase_timings: Vec<PhaseTiming>,
}

#[derive(Clone, Debug)]
pub struct BenchmarkSession {
    pub table_id: u32,
    pub player_count: usize,
    pub start_time: Instant,
    pub phase_starts: HashMap<String, Instant>,
    pub phase_durations: HashMap<String, u64>,
}

impl BenchmarkSession {
    pub fn new(table_id: u32, player_count: usize) -> Self {
        BenchmarkSession {
            table_id,
            player_count,
            start_time: Instant::now(),
            phase_starts: HashMap::new(),
            phase_durations: HashMap::new(),
        }
    }

    pub fn start_phase(&mut self, phase_name: &str) {
        self.phase_starts.insert(phase_name.to_string(), Instant::now());
    }

    pub fn end_phase(&mut self, phase_name: &str) {
        if let Some(start) = self.phase_starts.remove(phase_name) {
            let duration_ms = start.elapsed().as_millis() as u64;
            self.phase_durations.insert(phase_name.to_string(), duration_ms);
        }
    }

    pub fn finalize(self) -> DealBenchmark {
        let total_duration_ms = self.start_time.elapsed().as_millis() as u64;
        let mut phase_timings: Vec<PhaseTiming> = self
            .phase_durations
            .into_iter()
            .map(|(phase, duration_ms)| PhaseTiming {
                phase,
                duration_ms,
            })
            .collect();

        phase_timings.sort_by(|a, b| a.phase.cmp(&b.phase));

        DealBenchmark {
            table_id: self.table_id,
            player_count: self.player_count,
            total_duration_ms,
            phase_timings,
        }
    }
}

use std::sync::{Arc, Mutex};

pub type BenchmarkStore = Arc<Mutex<Vec<DealBenchmark>>>;

pub fn new_store() -> BenchmarkStore {
    Arc::new(Mutex::new(Vec::new()))
}

pub fn record_benchmark(store: &BenchmarkStore, benchmark: DealBenchmark) {
    if let Ok(mut benchmarks) = store.lock() {
        benchmarks.push(benchmark);
        if benchmarks.len() > 1000 {
            benchmarks.remove(0);
        }
    }
}

pub fn get_benchmarks(store: &BenchmarkStore, table_id: Option<u32>) -> Vec<DealBenchmark> {
    if let Ok(benchmarks) = store.lock() {
        match table_id {
            Some(id) => benchmarks.iter().filter(|b| b.table_id == id).cloned().collect(),
            None => benchmarks.clone(),
        }
    } else {
        Vec::new()
    }
}

pub fn get_benchmark_stats(benchmarks: &[DealBenchmark]) -> serde_json::Value {
    if benchmarks.is_empty() {
        return serde_json::json!({ "count": 0 });
    }

    let mut by_player_count: HashMap<usize, Vec<u64>> = HashMap::new();
    for benchmark in benchmarks {
        by_player_count
            .entry(benchmark.player_count)
            .or_default()
            .push(benchmark.total_duration_ms);
    }

    let mut stats = serde_json::Map::new();
    stats.insert("total_samples".to_string(), serde_json::json!(benchmarks.len()));

    for (player_count, durations) in by_player_count {
        let avg = durations.iter().sum::<u64>() / durations.len() as u64;
        let min = durations.iter().min().copied().unwrap_or(0);
        let max = durations.iter().max().copied().unwrap_or(0);

        stats.insert(
            format!("players_{}_count", player_count),
            serde_json::json!(durations.len()),
        );
        stats.insert(
            format!("players_{}_avg_ms", player_count),
            serde_json::json!(avg),
        );
        stats.insert(
            format!("players_{}_min_ms", player_count),
            serde_json::json!(min),
        );
        stats.insert(
            format!("players_{}_max_ms", player_count),
            serde_json::json!(max),
        );
    }

    serde_json::Value::Object(stats)
}
