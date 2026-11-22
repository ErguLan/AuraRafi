use raw_window_handle::RawWindowHandle;
use windows::core::Interface;
use windows::Win32::{
    Foundation::HWND,
    Graphics::{
        Direct3D12::*,
        Dxgi::{Common::*, *},
    },
};

use super::{dx_error, COLOR};
use crate::{Error, Result};

const FRAME_COUNT: u32 = 2;

pub(super) struct WindowTarget {
    hwnd: HWND,
    factory: IDXGIFactory6,
    swapchain: Option<IDXGISwapChain3>,
    extent: Option<[u32; 2]>,
}

impl WindowTarget {
    pub fn new(handle: RawWindowHandle) -> Result<Self> {
        let RawWindowHandle::Win32(handle) = handle else {
            return Err(Error::Invalid("Direct3D 12 requires a Win32 window"));
        };
        let hwnd = HWND(handle.hwnd.get() as *mut _);
        let factory =
            unsafe { CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0)) }.map_err(dx_error)?;
        Ok(Self {
            factory,
            hwnd,
            swapchain: None,
            extent: None,
        })
    }

    pub fn colors(
        &mut self,
        queue: &ID3D12CommandQueue,
        extent: [u32; 2],
    ) -> Result<Vec<ID3D12Resource>> {
        unsafe {
            match &self.swapchain {
                Some(chain) if self.extent != Some(extent) => {
                    chain
                        .ResizeBuffers(
                            FRAME_COUNT,
                            extent[0],
                            extent[1],
                            COLOR,
                            DXGI_SWAP_CHAIN_FLAG(0),
                        )
                        .map_err(dx_error)?;
                }
                Some(_) => {}
                None => {
                    let desc = DXGI_SWAP_CHAIN_DESC1 {
                        Width: extent[0],
                        Height: extent[1],
                        Format: COLOR,
                        SampleDesc: DXGI_SAMPLE_DESC {
                            Count: 1,
                            Quality: 0,
                        },
                        BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                        BufferCount: FRAME_COUNT,
                        Scaling: DXGI_SCALING_STRETCH,
                        SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
                        AlphaMode: DXGI_ALPHA_MODE_IGNORE,
                        ..Default::default()
                    };
                    let chain = self
                        .factory
                        .CreateSwapChainForHwnd(queue, self.hwnd, &desc, None, None::<&IDXGIOutput>)
                        .map_err(dx_error)?;
                    self.factory
                        .MakeWindowAssociation(self.hwnd, DXGI_MWA_NO_ALT_ENTER)
                        .map_err(dx_error)?;
                    self.swapchain = Some(chain.cast().map_err(dx_error)?);
                }
            }
            self.extent = Some(extent);
            let chain = self.swapchain.as_ref().ok_or(Error::SurfaceLost)?;
            (0..FRAME_COUNT)
                .map(|index| chain.GetBuffer(index).map_err(dx_error))
                .collect()
        }
    }

    pub fn current_index(&self) -> Result<usize> {
        let chain = self.swapchain.as_ref().ok_or(Error::SurfaceLost)?;
        Ok(unsafe { chain.GetCurrentBackBufferIndex() } as usize)
    }

    pub fn present(&self) -> Result<()> {
        let chain = self.swapchain.as_ref().ok_or(Error::SurfaceLost)?;
        unsafe { chain.Present(1, DXGI_PRESENT(0)) }
            .ok()
            .map_err(dx_error)
    }
}
