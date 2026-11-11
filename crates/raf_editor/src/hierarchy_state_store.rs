//! Bounded, coalesced persistence of editor-only hierarchy view state.

use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::panels::hierarchy_model::details::HierarchyViewState;

const SAVE_QUIET_PERIOD: Duration = Duration::from_millis(300);

struct Request {
    path: PathBuf,
    state: HierarchyViewState,
    ready_at: Instant,
}

#[derive(Default)]
struct Pending {
    latest: Option<Request>,
    writing: bool,
    force: bool,
    stopping: bool,
}

#[derive(Default)]
pub(crate) struct HierarchyStateWriter {
    shared: Arc<(Mutex<Pending>, Condvar)>,
    worker: Option<JoinHandle<()>>,
}

impl HierarchyStateWriter {
    pub fn submit(&mut self, path: PathBuf, state: HierarchyViewState) -> Result<(), String> {
        if self.worker.is_none() {
            let shared = Arc::clone(&self.shared);
            self.worker = Some(
                std::thread::Builder::new()
                    .name("raf-hierarchy-save".into())
                    .spawn(move || run(shared))
                    .map_err(|error| error.to_string())?,
            );
        }
        let (mutex, changed) = &*self.shared;
        let mut pending = mutex.lock().unwrap_or_else(|error| error.into_inner());
        // Session switches explicitly flush before changing paths. A single
        // replaceable slot is enough; scrolling cannot build an unbounded queue.
        pending.latest = Some(Request {
            path,
            state,
            ready_at: Instant::now() + SAVE_QUIET_PERIOD,
        });
        changed.notify_one();
        Ok(())
    }

    /// Only explicit persistence/session changes wait for durable completion.
    pub fn flush(&self) {
        let (mutex, changed) = &*self.shared;
        let mut pending = mutex.lock().unwrap_or_else(|error| error.into_inner());
        pending.force = true;
        changed.notify_one();
        while pending.latest.is_some() || pending.writing {
            pending = changed
                .wait(pending)
                .unwrap_or_else(|error| error.into_inner());
        }
        pending.force = false;
    }
}

impl Drop for HierarchyStateWriter {
    fn drop(&mut self) {
        let (mutex, changed) = &*self.shared;
        {
            let mut pending = mutex.lock().unwrap_or_else(|error| error.into_inner());
            pending.stopping = true;
            changed.notify_one();
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn run(shared: Arc<(Mutex<Pending>, Condvar)>) {
    let (mutex, changed) = &*shared;
    let mut pending = mutex.lock().unwrap_or_else(|error| error.into_inner());
    loop {
        if let Some(request) = pending.latest.as_ref() {
            let delay = request.ready_at.saturating_duration_since(Instant::now());
            if !pending.force && !pending.stopping && !delay.is_zero() {
                pending = changed
                    .wait_timeout(pending, delay)
                    .unwrap_or_else(|error| error.into_inner())
                    .0;
                continue;
            }
            let request = pending.latest.take().expect("pending request exists");
            pending.writing = true;
            drop(pending);
            let result = ron::ser::to_string(&request.state)
                .map_err(|error| error.to_string())
                .and_then(|text| {
                    raf_core::project_store::write_atomic(&request.path, text.as_bytes())
                });
            if let Err(error) = result {
                tracing::warn!(%error, path = %request.path.display(), "Hierarchy view state save failed");
            }
            pending = mutex.lock().unwrap_or_else(|error| error.into_inner());
            pending.writing = false;
            changed.notify_all();
        } else if pending.stopping {
            break;
        } else {
            pending = changed
                .wait(pending)
                .unwrap_or_else(|error| error.into_inner());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rafui_interaction_hierarchy_writer_flushes_latest_view_before_switching_sessions() {
        let directory =
            std::env::temp_dir().join(format!("raf-hierarchy-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        let first = directory.join("first.ron");
        let second = directory.join("second.ron");
        let mut writer = HierarchyStateWriter::default();
        for index in 0..32 {
            writer
                .submit(
                    first.clone(),
                    HierarchyViewState {
                        scroll: index as f32,
                        ..Default::default()
                    },
                )
                .unwrap();
        }
        writer.flush();
        let saved: HierarchyViewState =
            ron::from_str(&std::fs::read_to_string(&first).unwrap()).unwrap();
        assert_eq!(saved.scroll, 31.0);
        writer
            .submit(
                second.clone(),
                HierarchyViewState {
                    scroll: 8.0,
                    ..Default::default()
                },
            )
            .unwrap();
        writer.flush();
        drop(writer);
        let saved_second: HierarchyViewState =
            ron::from_str(&std::fs::read_to_string(&second).unwrap()).unwrap();
        assert_eq!(saved_second.scroll, 8.0);
        assert_eq!(
            std::fs::read_to_string(&first).unwrap(),
            ron::ser::to_string(&saved).unwrap()
        );
        std::fs::remove_file(first).unwrap();
        std::fs::remove_file(second).unwrap();
        let _ = std::fs::remove_file(directory.join(".first.ron.previous"));
        std::fs::remove_dir(directory).unwrap();
    }
}
