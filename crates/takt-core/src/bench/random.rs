use std::io::{Read, Seek, SeekFrom, Write};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use rand::Rng;

use crate::device::DeviceTarget;

use super::{
    BenchmarkContext, BenchmarkResult, BenchmarkType, SamplePoint, benchmark_file, build_result,
    bytes_to_mbps, emit_progress, open_rw, random_offset, reset_cursor, seeded_rng,
};

pub fn run_random_iops(
    _target: &DeviceTarget,
    context: &BenchmarkContext,
    progress: &mut impl FnMut(super::ProgressUpdate),
) -> Result<BenchmarkResult> {
    let path = benchmark_file(context, "random-iops.bin");
    let mut file = open_rw(&path)?;
    file.set_len(context.profile.random_file_bytes)
        .context("failed to size random IOPS file")?;
    reset_cursor(&mut file)?;

    let mut rng = seeded_rng();
    let mut buffer = vec![0x7F; context.profile.block_bytes];
    let mut latencies_ms = Vec::with_capacity(context.profile.random_operations as usize);
    let mut samples = Vec::new();
    let sample_interval = Duration::from_secs(1);
    let start = Instant::now();
    let mut last_sample = Instant::now();
    let mut interval_start = Instant::now();
    let mut interval_bytes = 0_u64;
    let mut operations = 0_u64;
    let mut processed_bytes = 0_u64;

    while operations < context.profile.random_operations {
        context.check_cancelled()?;
        let offset = random_offset(
            &mut rng,
            context.profile.random_file_bytes,
            context.profile.block_bytes,
        );
        file.seek(SeekFrom::Start(offset))
            .context("failed to seek random benchmark file")?;

        let op_start = Instant::now();
        if rng.random_bool(0.5) {
            file.write_all(&buffer)
                .context("failed to write random benchmark block")?;
        } else {
            file.read_exact(&mut buffer)
                .context("failed to read random benchmark block")?;
        }
        let latency = op_start.elapsed().as_secs_f64() * 1_000.0;
        latencies_ms.push(latency);
        operations += 1;
        processed_bytes += buffer.len() as u64;
        interval_bytes += buffer.len() as u64;

        if last_sample.elapsed() >= sample_interval
            || operations == context.profile.random_operations
        {
            let elapsed = start.elapsed();
            let sample_secs = interval_start.elapsed().as_secs_f64().max(f64::EPSILON);
            let sample_mbps = bytes_to_mbps(interval_bytes, sample_secs);
            samples.push(SamplePoint {
                seconds: elapsed.as_secs_f64(),
                throughput_mbps: sample_mbps,
            });
            emit_progress(
                progress,
                BenchmarkType::RandomIops,
                "probing",
                processed_bytes,
                Some(context.profile.random_operations * context.profile.block_bytes as u64),
                elapsed,
                sample_mbps,
            );
            last_sample = Instant::now();
            interval_start = Instant::now();
            interval_bytes = 0;
        }
    }

    file.sync_all()
        .context("failed to flush random benchmark file")?;
    let duration = start.elapsed();
    let iops = operations as f64 / duration.as_secs_f64().max(f64::EPSILON);

    Ok(build_result(
        BenchmarkType::RandomIops,
        processed_bytes,
        duration,
        samples,
        Some(iops),
        Some(&latencies_ms),
    ))
}
