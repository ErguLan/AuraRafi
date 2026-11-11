//! Shared retained table tracks for editor reports. Fixed columns and the
//! message column use the same layout contract in headings and data rows.
use crate::*;

pub fn table_cell_layout(width: f32, height: f32) -> UiLayout {
    if width > 0.0 {
        UiLayout::fixed(width, height)
    } else {
        UiLayout::fixed(0.0, height).fill_width().grow(1.0)
    }
}

pub fn table_heading(id: &str, columns: &[(&str, f32)], tokens: UiTokens) -> UiNode {
    let mut row = UiNode::new(id, UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 4.0,
            padding: UiSpacing::xy(6.0, 2.0),
            ..UiLayout::fixed(0.0, 32.0).fill_width()
        })
        .with_style(UiStyle::transparent().with_fill(tokens.surface_raised));
    for (index, (key, width)) in columns.iter().enumerate() {
        row = row.with_child(
            UiNode::new(format!("{id}.{index}"), UiNodeKind::Label)
                .with_text_key(*key)
                .with_text_style(UiTextStyle {
                    role: UiTextRole::Label,
                    size_px: 13.0,
                    line_height_px: 18.0,
                    ..UiTextStyle::button(tokens.text_muted)
                })
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_layout(table_cell_layout(*width, 22.0)),
        );
    }
    row
}
