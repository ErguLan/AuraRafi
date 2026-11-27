use crate::{
    AdapterInfo, Backend, Error, Frame, FrameMetrics, FramePlan, ResourcePatch, Result, Workload,
};

/// CPU-side executor for testing admission and submission of validated data.
pub struct Recorder {
    info: AdapterInfo,
    pub submissions: u64,
}

impl Default for Recorder {
    fn default() -> Self {
        Self {
            info: AdapterInfo {
                backend: "recorder",
                name: "CPU".into(),
                driver: String::new(),
                hardware: false,
                max_texture: 16384,
                max_buffer: u32::MAX.into(),
                gpu_timestamps: false,
            },
            submissions: 0,
        }
    }
}

impl Backend for Recorder {
    type Resident = u32;

    fn info(&self) -> &AdapterInfo {
        &self.info
    }
    fn configure(&mut self, _: &FramePlan) -> Result<()> {
        Ok(())
    }
    fn upload(&mut self, workload: &Workload) -> Result<u32> {
        Ok(workload.batches.len() as u32)
    }

    fn supports_frame_instance_patches(&self) -> bool {
        true
    }

    unsafe fn patch(&mut self, _: &mut u32, _: ResourcePatch<'_>) -> Result<()> {
        Ok(())
    }

    fn draw(&mut self, calls: &u32, plan: &FramePlan, _: &Frame) -> Result<FrameMetrics> {
        if plan.graph.order.as_slice() != [0, 1, 2] {
            return Err(Error::Invalid("unexpected pass order"));
        }
        self.submissions += 1;
        Ok(FrameMetrics {
            draw_calls: *calls,
            submits: 1,
            ..Default::default()
        })
    }

    fn wait(&mut self) -> Result<()> {
        Ok(())
    }
    fn capture(&mut self) -> Result<Vec<u8>> {
        Err(Error::Invalid("the recorder has no pixels"))
    }
}
