use std::fs::File;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use crate::device::DeviceTarget;

use super::{
    BenchmarkContext, BenchmarkResult, BenchmarkType, SamplePoint, benchmark_file, build_result,
    bytes_to_mbps, emit_progress, write_chunk,
};

pub fn run_sustained_write(
    _target: &DeviceTarget,
    context: &BenchmarkContext,
    progress: &mut impl FnMut(super::ProgressUpdate),
) -> Result<BenchmarkResult> {
    let buffer = vec![0x3C; context.profile.chunk_bytes];
    let sample_interval = Duration::from_secs(1);
    let run_duration = Duration::from_secs(context.profile.sustained_seconds);
    let start = Instant::now();
    let mut last_sample = Instant::now();
    let mut interval_start = Instant::now();
    let mut interval_bytes = 0_u64;
    let mut total_bytes = 0_u64;
    let mut file_index = 0_usize;
    let mut file = create_segment(context, file_index)?;
    let mut segment_bytes = 0_u64;
    let segment_limit = 64_u64 * 1024 * 1024;
    let mut samples = Vec::new();

    while start.elapsed() < run_duration {
        context.check_cancelled()?;

        if segment_bytes >= segment_limit {
            file.sync_all().ok();
            file_index += 1;
            file = create_segment(context, file_index)?;
            segment_bytes = 0;
        }

        write_chunk(&mut file, &buffer, buffer.len())?;
        segment_bytes += buffer.len() as u64;
        total_bytes += buffer.len() as u64;
        interval_bytes += buffer.len() as u64;

        if last_sample.elapsed() >= sample_interval {
            let elapsed = start.elapsed();
            let sample_secs = interval_start.elapsed().as_secs_f64().max(f64::EPSILON);
            let sample_mbps = bytes_to_mbps(interval_bytes, sample_secs);
            samples.push(SamplePoint {
                seconds: elapsed.as_secs_f64(),
                throughput_mbps: sample_mbps,
            });
            emit_progress(
                progress,
                BenchmarkType::SustainedWrite,
                "sustaining",
                total_bytes,
                None,
                elapsed,
                sample_mbps,
            );
            last_sample = Instant::now();
            interval_start = Instant::now();
            interval_bytes = 0;
        }
    }

    file.sync_all()
        .context("failed to flush sustained write benchmark")?;
    Ok(build_result(
        BenchmarkType::SustainedWrite,
        total_bytes,
        start.elapsed(),
        samples,
        None,
        None,
    ))
}

fn create_segment(context: &BenchmarkContext, index: usize) -> Result<File> {
    let path = benchmark_file(context, &format!("sustained-{index:03}.bin"));
    let file = File::create(&path)
        .with_context(|| format!("failed to create sustained segment {}", path.display()))?;
    Ok(file)
}
