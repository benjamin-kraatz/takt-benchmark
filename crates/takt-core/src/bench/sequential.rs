use std::fs::File;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use crate::device::DeviceTarget;

use super::{
    BenchmarkContext, BenchmarkResult, BenchmarkType, SamplePoint, benchmark_file, build_result,
    bytes_to_mbps, emit_progress, ensure_fixture, read_exact_chunk, sample_tick, write_chunk,
};

pub fn run_sequential_write(
    _target: &DeviceTarget,
    context: &BenchmarkContext,
    progress: &mut impl FnMut(super::ProgressUpdate),
) -> Result<BenchmarkResult> {
    let path = benchmark_file(context, "sequential-write.bin");
    let mut file =
        File::create(&path).with_context(|| format!("failed to create {}", path.display()))?;
    let total_bytes = context.profile.sequential_bytes;
    let mut written = 0_u64;
    let buffer = vec![0xA5; context.profile.chunk_bytes];
    let mut samples = Vec::new();
    let sample_interval = Duration::from_millis(250);
    let start = Instant::now();
    let mut last_tick = Instant::now();
    let mut interval_start = Instant::now();
    let mut interval_bytes = 0_u64;

    while written < total_bytes {
        context.check_cancelled()?;
        let next_chunk = (total_bytes - written).min(buffer.len() as u64) as usize;
        write_chunk(&mut file, &buffer, next_chunk)?;
        written += next_chunk as u64;
        interval_bytes += next_chunk as u64;

        if sample_tick(&mut last_tick, sample_interval) || written == total_bytes {
            let elapsed = start.elapsed();
            let sample_secs = interval_start.elapsed().as_secs_f64().max(f64::EPSILON);
            let sample_mbps = bytes_to_mbps(interval_bytes, sample_secs);
            samples.push(SamplePoint {
                seconds: elapsed.as_secs_f64(),
                throughput_mbps: sample_mbps,
            });
            emit_progress(
                progress,
                BenchmarkType::SequentialWrite,
                "writing",
                written,
                Some(total_bytes),
                elapsed,
                sample_mbps,
            );
            interval_start = Instant::now();
            interval_bytes = 0;
        }
    }

    file.sync_all()
        .context("failed to flush sequential write benchmark")?;
    Ok(build_result(
        BenchmarkType::SequentialWrite,
        written,
        start.elapsed(),
        samples,
        None,
        None,
    ))
}

pub fn run_sequential_read(
    _target: &DeviceTarget,
    context: &BenchmarkContext,
    progress: &mut impl FnMut(super::ProgressUpdate),
) -> Result<BenchmarkResult> {
    let path = benchmark_file(context, "sequential-read.bin");
    ensure_fixture(
        &path,
        context.profile.sequential_bytes,
        context.profile.chunk_bytes,
    )?;
    let mut file =
        File::open(&path).with_context(|| format!("failed to open {}", path.display()))?;
    let total_bytes = context.profile.sequential_bytes;
    let mut read = 0_u64;
    let mut buffer = vec![0_u8; context.profile.chunk_bytes];
    let mut samples = Vec::new();
    let sample_interval = Duration::from_millis(250);
    let start = Instant::now();
    let mut last_tick = Instant::now();
    let mut interval_start = Instant::now();
    let mut interval_bytes = 0_u64;

    while read < total_bytes {
        context.check_cancelled()?;
        let next_chunk = (total_bytes - read).min(buffer.len() as u64) as usize;
        read_exact_chunk(&mut file, &mut buffer, next_chunk)?;
        read += next_chunk as u64;
        interval_bytes += next_chunk as u64;

        if sample_tick(&mut last_tick, sample_interval) || read == total_bytes {
            let elapsed = start.elapsed();
            let sample_secs = interval_start.elapsed().as_secs_f64().max(f64::EPSILON);
            let sample_mbps = bytes_to_mbps(interval_bytes, sample_secs);
            samples.push(SamplePoint {
                seconds: elapsed.as_secs_f64(),
                throughput_mbps: sample_mbps,
            });
            emit_progress(
                progress,
                BenchmarkType::SequentialRead,
                "reading",
                read,
                Some(total_bytes),
                elapsed,
                sample_mbps,
            );
            interval_start = Instant::now();
            interval_bytes = 0;
        }
    }

    Ok(build_result(
        BenchmarkType::SequentialRead,
        read,
        start.elapsed(),
        samples,
        None,
        None,
    ))
}
