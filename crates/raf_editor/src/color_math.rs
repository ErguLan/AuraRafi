use raf_core::scene::NodeColor;
use raf_ui::UiColorPicker;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct HsvColor {
    pub hue: f32,
    pub saturation: f32,
    pub value: f32,
}

pub(crate) fn rgb_to_hsv(color: NodeColor) -> HsvColor {
    let red = f32::from(color.r) / 255.0;
    let green = f32::from(color.g) / 255.0;
    let blue = f32::from(color.b) / 255.0;
    let max = red.max(green).max(blue);
    let min = red.min(green).min(blue);
    let delta = max - min;

    let hue = if delta <= f32::EPSILON {
        0.0
    } else if (max - red).abs() <= f32::EPSILON {
        (60.0 * ((green - blue) / delta)).rem_euclid(360.0)
    } else if (max - green).abs() <= f32::EPSILON {
        60.0 * ((blue - red) / delta + 2.0)
    } else {
        60.0 * ((red - green) / delta + 4.0)
    };

    HsvColor {
        hue,
        saturation: if max <= f32::EPSILON {
            0.0
        } else {
            delta / max
        },
        value: max,
    }
}

pub(crate) fn hsv_to_node_color(hsv: HsvColor, alpha: u8) -> NodeColor {
    let [red, green, blue] = UiColorPicker::hsv_to_rgb_bytes(hsv.hue, hsv.saturation, hsv.value);

    NodeColor::rgba(red, green, blue, alpha)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb_hsv_round_trip_preserves_primary_colors() {
        for color in [
            NodeColor::rgba(255, 0, 0, 17),
            NodeColor::rgba(0, 255, 0, 34),
            NodeColor::rgba(0, 0, 255, 51),
            NodeColor::rgba(160, 110, 60, 255),
        ] {
            let hsv = rgb_to_hsv(color);
            assert_eq!(hsv_to_node_color(hsv, color.a), color);
        }
    }

    #[test]
    fn gray_colors_have_zero_saturation_and_stable_hue() {
        let hsv = rgb_to_hsv(NodeColor::rgba(128, 128, 128, 255));
        assert_eq!(hsv.hue, 0.0);
        assert_eq!(hsv.saturation, 0.0);
        assert!((hsv.value - 128.0 / 255.0).abs() < 0.001);
    }
}
