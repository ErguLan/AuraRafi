//! Small bootstrap screen shared by the isolated window and offscreen tooling.

use super::{Command, FrameUniforms, PacketKey, Paint, PaintQuad, ProductDevice, ProductPacket};
use crate::{Backend, Error, FrameMetrics, Result, Texture};

const STARTUP_PACKET: PacketKey = PacketKey(0);
const CLIP: [f32; 4] = [0., 0., 1., 1.];
const ATLAS_SIZE: [u32; 2] = [17, 7];
const GLYPHS: [[u8; 7]; 3] = [
    [14, 17, 17, 31, 17, 17, 17], // A
    [14, 17, 16, 23, 17, 17, 14], // G
    [30, 17, 17, 30, 17, 17, 30], // B
];

/// Draw the first screen through the product contract. Scale and extent are
/// part of the revision. Callers increase revision on either change.
pub fn draw<B: Backend>(
    device: &mut ProductDevice<B>,
    extent: [u32; 2],
    scale_factor: f64,
    revision: u64,
) -> Result<FrameMetrics> {
    if extent.contains(&0) || !scale_factor.is_finite() || scale_factor <= 0. {
        return Err(Error::Invalid("invalid startup extent or scale"));
    }
    let handle = device.prepare(STARTUP_PACKET, revision, || {
        screen(extent, scale_factor as f32)
    })?;
    device.draw(handle, &FrameUniforms::default())
}

fn screen([width, height]: [u32; 2], scale: f32) -> ProductPacket {
    let width = width as f32;
    let height = height as f32;
    let scale = scale.min(width / 192.).min(height / 100.);
    let rect = |x: f32, y: f32, w: f32, h: f32| PaintQuad {
        rect: [
            0.5 + x * scale / width,
            0.5 + y * scale / height,
            w * scale / width,
            h * scale / height,
        ],
        clip: CLIP,
        uv_rect: CLIP,
        color: [1.; 4],
    };
    let solid = |mut quad: PaintQuad, color| {
        quad.color = color;
        Command::Paint(Paint::Solid(quad))
    };
    let dot = Texture {
        size: [2, 2],
        rgba: vec![
            73, 186, 167, 255, 135, 232, 202, 255, 135, 232, 202, 255, 73, 186, 167, 255,
        ],
    };
    let mut atlas = vec![255; (ATLAS_SIZE[0] * ATLAS_SIZE[1] * 4) as usize];
    for (glyph, rows) in GLYPHS.iter().enumerate() {
        for (y, row) in rows.iter().enumerate() {
            for x in 0..5 {
                let pixel = y * ATLAS_SIZE[0] as usize + glyph * 6 + x;
                atlas[pixel * 4 + 3] = if (row >> (4 - x)) & 1 != 0 { 255 } else { 0 };
            }
        }
    }
    for y in 0..ATLAS_SIZE[1] as usize {
        for x in [5, 11] {
            atlas[(y * ATLAS_SIZE[0] as usize + x) * 4 + 3] = 0;
        }
    }
    ProductPacket {
        textures: vec![
            Texture {
                size: ATLAS_SIZE,
                rgba: atlas,
            },
            dot,
        ],
        commands: vec![
            Command::Clear([0.032, 0.045, 0.064, 1.]),
            solid(rect(-88., -38., 176., 76.), [0.11, 0.15, 0.20, 1.]),
            solid(rect(-88., -38., 3., 76.), [0.28, 0.73, 0.66, 1.]),
            Command::Paint(Paint::Image {
                quad: rect(-68., -10., 20., 20.),
                texture: 1,
            }),
            Command::Paint(Paint::Text {
                quad: rect(-29., -10., 51., 21.),
                atlas: 0,
            }),
        ],
        ..ProductPacket::default()
    }
}
