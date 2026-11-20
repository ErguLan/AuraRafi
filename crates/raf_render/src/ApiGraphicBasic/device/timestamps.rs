use std::sync::{mpsc, Arc};

use super::backend_wgpu::wgpu_timestamp_features;

pub(super) const GPU_TIMESTAMP_QUERY_BYTES: u64 = 2 * std::mem::size_of::<u64>() as u64;
const GPU_TIMESTAMP_READBACK_SLOTS: usize = 3;

struct GpuTimestampReadback {
    buffer: Arc<wgpu::Buffer>,
    pending: bool,
}

struct GpuTimestampCompletion {
    slot: usize,
    succeeded: bool,
}

/// Non-blocking GPU frame timing. Results are read several frames later so
/// diagnostics never stall rendering just to obtain a number for the HUD.
pub(super) struct GpuTimestampState {
    pub(super) query_set: Arc<wgpu::QuerySet>,
    pub(super) resolve_buffer: Arc<wgpu::Buffer>,
    readbacks: Vec<GpuTimestampReadback>,
    completion_tx: mpsc::Sender<GpuTimestampCompletion>,
    completion_rx: mpsc::Receiver<GpuTimestampCompletion>,
    timestamp_period_ns: f32,
    next_slot: usize,
    pub(super) last_gpu_ms: f32,
    pub(super) has_sample: bool,
}

impl GpuTimestampState {
    pub(super) fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Option<Self> {
        if !device.features().contains(wgpu_timestamp_features()) {
            return None;
        }

        let query_set = Arc::new(device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("ApiGraphicBasic.SceneTimestampQueries"),
            ty: wgpu::QueryType::Timestamp,
            count: 2,
        }));
        let resolve_buffer = Arc::new(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ApiGraphicBasic.SceneTimestampResolve"),
            size: GPU_TIMESTAMP_QUERY_BYTES,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        }));
        let readbacks = (0..GPU_TIMESTAMP_READBACK_SLOTS)
            .map(|_| GpuTimestampReadback {
                buffer: Arc::new(device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("ApiGraphicBasic.SceneTimestampReadback"),
                    size: GPU_TIMESTAMP_QUERY_BYTES,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                })),
                pending: false,
            })
            .collect();
        let (completion_tx, completion_rx) = mpsc::channel();

        Some(Self {
            query_set,
            resolve_buffer,
            readbacks,
            completion_tx,
            completion_rx,
            timestamp_period_ns: queue.get_timestamp_period(),
            next_slot: 0,
            last_gpu_ms: 0.0,
            has_sample: false,
        })
    }

    pub(super) fn collect_ready(&mut self, device: &wgpu::Device) {
        if self.readbacks.iter().any(|readback| readback.pending) {
            let _ = device.poll(wgpu::Maintain::Poll);
        }

        while let Ok(completion) = self.completion_rx.try_recv() {
            let Some(readback) = self.readbacks.get_mut(completion.slot) else {
                continue;
            };
            if completion.succeeded {
                let mapped = readback.buffer.slice(..).get_mapped_range();
                if mapped.len() >= GPU_TIMESTAMP_QUERY_BYTES as usize {
                    let start = u64::from_le_bytes(
                        mapped[0..8]
                            .try_into()
                            .expect("timestamp start has a fixed byte width"),
                    );
                    let end = u64::from_le_bytes(
                        mapped[8..16]
                            .try_into()
                            .expect("timestamp end has a fixed byte width"),
                    );
                    if end >= start {
                        self.last_gpu_ms =
                            (end - start) as f32 * self.timestamp_period_ns / 1_000_000.0;
                        self.has_sample = true;
                    }
                }
                drop(mapped);
                readback.buffer.unmap();
            }
            readback.pending = false;
        }
    }

    pub(super) fn reserve_readback(&mut self) -> Option<(usize, Arc<wgpu::Buffer>)> {
        for offset in 0..self.readbacks.len() {
            let slot = (self.next_slot + offset) % self.readbacks.len();
            if !self.readbacks[slot].pending {
                self.readbacks[slot].pending = true;
                self.next_slot = (slot + 1) % self.readbacks.len();
                return Some((slot, Arc::clone(&self.readbacks[slot].buffer)));
            }
        }
        None
    }

    pub(super) fn map_readback(&self, slot: usize) {
        let Some(readback) = self.readbacks.get(slot) else {
            return;
        };
        let completion_tx = self.completion_tx.clone();
        readback
            .buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = completion_tx.send(GpuTimestampCompletion {
                    slot,
                    succeeded: result.is_ok(),
                });
            });
    }
}
