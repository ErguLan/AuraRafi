//! One cooperative analysis worker per Electronics editor, with one pending job.
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Condvar, Mutex};

use raf_electronics::{CadSurfaceKind, DrcReport, PcbLayout, Schematic, SimulationResults};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AnalysisKind {
    Drc,
    Simulation,
}

pub(super) enum AnalysisResult {
    Drc(DrcReport),
    Simulation(SimulationResults),
}

pub(super) struct AnalysisTask {
    kind: AnalysisKind,
    pub(super) surface: CadSurfaceKind,
    cancel: Arc<AtomicBool>,
    receiver: Receiver<AnalysisResult>,
}

struct Job {
    kind: AnalysisKind,
    schematic: Schematic,
    pcb: Option<PcbLayout>,
    cancel: Arc<AtomicBool>,
    sender: Sender<AnalysisResult>,
}

#[derive(Default)]
struct WorkerState {
    pending: Option<Job>,
    running: Option<Arc<AtomicBool>>,
    stopped: bool,
}

pub(super) struct AnalysisWorker {
    state: Arc<(Mutex<WorkerState>, Condvar)>,
    available: bool,
}

impl AnalysisWorker {
    pub(super) fn new() -> Self {
        let state = Arc::new((Mutex::new(WorkerState::default()), Condvar::new()));
        let worker = Arc::clone(&state);
        let available = std::thread::Builder::new()
            .name("raf-electronics-analysis".into())
            .spawn(move || loop {
                let job = {
                    let (lock, wake) = &*worker;
                    let mut state = lock.lock().unwrap_or_else(|e| e.into_inner());
                    while state.pending.is_none() && !state.stopped {
                        state = wake.wait(state).unwrap_or_else(|e| e.into_inner());
                    }
                    if state.stopped {
                        break;
                    }
                    let job = state.pending.take().expect("pending job");
                    state.running = Some(Arc::clone(&job.cancel));
                    job
                };
                let result = match job.kind {
                    AnalysisKind::Drc => match job.pcb {
                        Some(pcb) => pcb.run_drc_cancellable(&job.cancel),
                        None => {
                            raf_electronics::drc::run_drc_cancellable(&job.schematic, &job.cancel)
                        }
                    }
                    .map(AnalysisResult::Drc),
                    AnalysisKind::Simulation => {
                        raf_electronics::simulation::simulate_dc_cancellable(
                            &job.schematic,
                            &job.cancel,
                        )
                        .map(AnalysisResult::Simulation)
                    }
                };
                if !job.cancel.load(Ordering::Acquire) {
                    if let Some(result) = result {
                        let _ = job.sender.send(result);
                    }
                }
                worker.0.lock().unwrap_or_else(|e| e.into_inner()).running = None;
            })
            .is_ok();
        Self { state, available }
    }

    pub(super) fn submit(
        &self,
        kind: AnalysisKind,
        schematic: Schematic,
        pcb: Option<PcbLayout>,
    ) -> AnalysisTask {
        let (sender, receiver) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let surface = if pcb.is_some() {
            CadSurfaceKind::Pcb
        } else {
            CadSurfaceKind::Schematic
        };
        if self.available {
            let mut state = self.state.0.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(running) = &state.running {
                running.store(true, Ordering::Release);
            }
            if let Some(old) = state.pending.replace(Job {
                kind,
                schematic,
                pcb,
                cancel: Arc::clone(&cancel),
                sender,
            }) {
                old.cancel.store(true, Ordering::Release);
            }
            self.state.1.notify_one();
        }
        AnalysisTask {
            kind,
            surface,
            cancel,
            receiver,
        }
    }
}

impl Drop for AnalysisWorker {
    fn drop(&mut self) {
        let mut state = self.state.0.lock().unwrap_or_else(|e| e.into_inner());
        state.stopped = true;
        if let Some(running) = &state.running {
            running.store(true, Ordering::Release);
        }
        if let Some(pending) = state.pending.take() {
            pending.cancel.store(true, Ordering::Release);
        }
        self.state.1.notify_one();
    }
}

impl AnalysisTask {
    pub(super) fn kind(&self) -> AnalysisKind {
        self.kind
    }
    pub(super) fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }
    pub(super) fn try_result(&self) -> Result<Option<AnalysisResult>, ()> {
        match self.receiver.try_recv() {
            Ok(result) => Ok(Some(result)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(()),
        }
    }
}

impl Drop for AnalysisTask {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    #[test]
    fn electronics_worker_cancels_a_large_job_and_serves_the_latest_request() {
        let worker = AnalysisWorker::new();
        assert!(worker.available);
        let mut large = Schematic::new("cancel");
        for i in 0..25000 {
            let x = i as f32 * 10.;
            large.add_wire(glam::Vec2::new(x, 0.), glam::Vec2::new(x + 10., 0.), "");
        }
        let first = worker.submit(AnalysisKind::Drc, large, None);
        first.cancel();
        let mut previous = first;
        for _ in 0..100 {
            previous = worker.submit(AnalysisKind::Simulation, Schematic::new("replace"), None);
        }
        previous.cancel();
        let mut pcb = PcbLayout::new("latest");
        pcb.board_outline.points.clear();
        let latest = worker.submit(AnalysisKind::Drc, Schematic::new("latest"), Some(pcb));
        let AnalysisResult::Drc(report) = latest
            .receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("latest analysis completed")
        else {
            panic!("wrong analysis kind")
        };
        assert!(report
            .errors
            .iter()
            .any(|issue| issue.rule == "invalid_board_outline"));
        assert_eq!(latest.surface, CadSurfaceKind::Pcb);
    }
}
