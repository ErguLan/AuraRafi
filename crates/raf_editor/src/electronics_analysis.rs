//! Background analysis jobs for the native Electronics editor.
//!
//! DRC and DC simulation are pure functions over a schematic. The UI owns a
//! small cancellable job handle and receives only the finished value; no
//! renderer or RafUI type crosses this boundary.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::Arc;
use std::thread;

use raf_electronics::{DrcReport, Schematic, SimulationResults};

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
    cancel: Arc<AtomicBool>,
    receiver: Receiver<AnalysisResult>,
}

impl AnalysisTask {
    pub(super) fn spawn(kind: AnalysisKind, schematic: Schematic) -> Self {
        let (sender, receiver) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        thread::spawn(move || {
            let result = match kind {
                AnalysisKind::Drc => AnalysisResult::Drc(schematic.run_drc()),
                AnalysisKind::Simulation => AnalysisResult::Simulation(schematic.simulate_dc()),
            };
            if !worker_cancel.load(Ordering::Acquire) {
                let _ = sender.send(result);
            }
        });
        Self {
            kind,
            cancel,
            receiver,
        }
    }

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
