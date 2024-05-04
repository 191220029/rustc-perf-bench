use crate::benchmark::black_box;
use crate::comm::messages::BenchmarkStats;
use perf_event::events::Hardware;
use perf_event::{Builder, Counter, Group};
use std::time::Instant;

/// A collection of CPU performance counters.
/// The counters are optional, because some CPUs are not able to record them.
struct Counters {
    cycles: Option<Counter>,
    instructions: Option<Counter>,
    branch_misses: Option<Counter>,
    cache_misses: Option<Counter>,
    cache_references: Option<Counter>,
}

/// Benchmarks a single function generated by `benchmark_constructor`.
/// The function is executed twice, once to gather wall-time measurement and the second time to
/// gather perf. counters.
pub fn benchmark_function<F: Fn() -> Bench, R, Bench: FnOnce() -> R>(
    benchmark_constructor: &F,
) -> anyhow::Result<BenchmarkStats> {
    let mut group = create_group()?;
    let counters = prepare_counters(&mut group)?;

    // Measure perf. counters.
    let func = benchmark_constructor();

    // Do not act on the return value to avoid including the branch in the measurement
    let enable_ret = group.enable();
    let output = func();
    group.disable()?;

    // Try to avoid optimizing the result out.
    black_box(output);

    // Check if we have succeeded before
    enable_ret?;

    let measurement = group.read()?;

    // Measure wall time.
    let func = benchmark_constructor();

    let start = Instant::now();
    let output = func();
    let duration = start.elapsed();

    // Try to avoid optimizing the result out.
    black_box(output);

    let result = BenchmarkStats {
        cycles: counters.cycles.map(|c| measurement[&c]),
        instructions: counters.instructions.map(|c| measurement[&c]),
        branch_misses: counters.branch_misses.map(|c| measurement[&c]),
        cache_misses: counters.cache_misses.map(|c| measurement[&c]),
        cache_references: counters.cache_references.map(|c| measurement[&c]),
        wall_time: duration,
    };
    Ok(result)
}

fn create_group() -> anyhow::Result<Group> {
    match Group::new() {
        Ok(group) => Ok(group),
        Err(error) => {
            let path = "/proc/sys/kernel/perf_event_paranoid";
            let level = std::fs::read_to_string(path).unwrap_or_else(|_| "unknown".to_string());
            let level = level.trim();
            Err(anyhow::anyhow!(
                "Cannot create perf_event group ({:?}). Current value of {} is {}.
Try lowering it with `sudo bash -c 'echo -1 > /proc/sys/kernel/perf_event_paranoid'`.",
                error,
                path,
                level
            ))
        }
    }
}

fn prepare_counters(group: &mut Group) -> anyhow::Result<Counters> {
    let mut add_event = |event: Hardware| match Builder::new().group(group).kind(event).build() {
        Ok(counter) => Some(counter),
        Err(error) => {
            log::warn!(
                "Could not add counter {:?}: {:?}. Maybe the CPU doesn't support it?",
                event,
                error
            );
            None
        }
    };

    let cycles = add_event(Hardware::CPU_CYCLES);
    let instructions = add_event(Hardware::INSTRUCTIONS);
    let branch_misses = add_event(Hardware::BRANCH_MISSES);
    let cache_misses = add_event(Hardware::CACHE_MISSES);
    let cache_references = add_event(Hardware::CACHE_REFERENCES);

    Ok(Counters {
        cycles,
        instructions,
        branch_misses,
        cache_misses,
        cache_references,
    })
}