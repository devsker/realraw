//! Demo task graphs for showcasing the [`task`](crate::task) system.
//!
//! Future real features (file import, catalog open, export) will plug into
//! the same `TaskManager` API these demos use.

use std::time::Duration;

use crate::task::{GroupId, Task, TaskContext, TaskManager};

/// Build and start a realistic import batch in `mgr`.
///
/// Exercises every interesting bit of the task system in one shot:
/// * parallel work (5 decodes start together)
/// * `depends_on` / `depends_on_all` (thumbnails, previews, metadata)
/// * nested groups (Decode / Thumbnails / Metadata / Preview under a batch)
/// * auto-grouping on the loose "Sidecar::..." / "Build proxy::..." tasks
///
/// Returns the id of the root group so the caller can highlight / cancel
/// the whole batch as one unit.
pub fn run_import_batch(mgr: &mut TaskManager, batch_id: u32) -> GroupId {
    let outer = mgr.add_group(format!("Import batch #{batch_id}"), None);
    let decode = mgr.add_group("Decode RAW", Some(outer));
    let thumbs = mgr.add_group("Thumbnails", Some(outer));
    let meta = mgr.add_group("Metadata", Some(outer));
    let preview = mgr.add_group("Preview", Some(outer));

    // 5 parallel decode tasks.
    let mut decode_ids = Vec::with_capacity(5);
    for i in 0..5 {
        let task = Task::new(
            format!("Decode file {i}.cr2"),
            "RAW demosaic + white balance",
        )
        .group(decode)
        .work(|ctx: &TaskContext| demo_work(ctx, 25, "decoding"));
        decode_ids.push(mgr.add_task(task));
    }

    // Thumbnails depend on their corresponding decode.
    let mut thumb_ids = Vec::with_capacity(decode_ids.len());
    for (i, dep) in decode_ids.iter().enumerate() {
        let task = Task::new(format!("Thumbnail {i}"), "256x256 preview")
            .group(thumbs)
            .depends_on(*dep)
            .work(|ctx: &TaskContext| demo_work(ctx, 15, "thumbnailing"));
        thumb_ids.push(mgr.add_task(task));
    }

    // Metadata extraction depends on every decode.
    let meta_task = Task::new("Extract EXIF / IPTC", "Build keyword index")
        .group(meta)
        .depends_on_all(decode_ids.iter().copied())
        .work(|ctx: &TaskContext| demo_work(ctx, 30, "extracting"));
    let meta_id = mgr.add_task(meta_task);

    // Previews depend on their thumbnail + the metadata task.
    for (i, dep) in thumb_ids.iter().enumerate() {
        let task = Task::new(format!("Preview {i}"), "Build full-res preview")
            .group(preview)
            .depends_on(*dep)
            .depends_on(meta_id)
            .work(|ctx: &TaskContext| demo_work(ctx, 20, "previewing"));
        mgr.add_task(task);
    }

    // Loose (ungrouped) tasks to exercise the auto-grouping UI.
    for j in 0..3 {
        let task = Task::new(format!("Sidecar::verify {j}"), "Hash + checksum")
            .work(|ctx: &TaskContext| demo_work(ctx, 10, "verifying"));
        mgr.add_task(task);
    }
    for j in 0..2 {
        let task = Task::new(format!("Build proxy::video {j}"), "Transcode proxy")
            .work(|ctx: &TaskContext| demo_work(ctx, 40, "transcoding"));
        mgr.add_task(task);
    }

    mgr.start();
    outer
}

/// Demo work closure: tick `ticks` times, reporting progress + a status
/// message and respecting cancellation.
pub fn demo_work(ctx: &TaskContext, ticks: u32, label: &str) -> Result<(), String> {
    for i in 0..ticks {
        if ctx.is_cancelled() {
            return Err("cancelled".to_string());
        }
        ctx.set_message(format!("{label} step {}/{}", i + 1, ticks));
        ctx.set_progress(i as f32 / ticks as f32);
        std::thread::sleep(Duration::from_millis(120));
    }
    ctx.set_progress(1.0);
    Ok(())
}
