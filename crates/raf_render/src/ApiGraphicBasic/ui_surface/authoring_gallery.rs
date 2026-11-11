//! Real component recipes for native hosts and offscreen review. This fixture
//! does not install an editor panel or execute its emitted domain commands.
use super::UiSurface;
use raf_ui::*;

pub fn component_gallery(palette: StudioUiPalette) -> UiSurface {
    let kit = UiComponents::for_palette(palette);
    let t = kit.tokens;
    let m = kit.metrics;
    let title = UiNode::new("components.title", UiNodeKind::Label)
        .with_text_key("ui.gallery.components")
        .with_text_style(UiTextStyle::panel_title(t.text).with_size(18.0))
        .with_layout(UiLayout::fit_content().fill_width());
    let description = UiNode::new("components.description", UiNodeKind::Label)
        .with_text_key("ui.gallery.description")
        .with_text_style(UiTextStyle::body(t.text_muted))
        .with_layout(UiLayout::fit_content().fill_width());
    let command = |name: &str| UiAction::Command { name: name.into() };
    let actions = kit.toolbar(
        "components.actions",
        [
            kit.button(
                "components.save",
                "app.save_menu",
                command("project.save"),
                UiButtonVariant::Accent,
            )
            .with_icon(UiIcon::new(UiIconId::File).with_size(UiIconSize::Toolbar)),
            kit.button(
                "components.secondary",
                "ui.gallery.secondary",
                command("gallery.secondary"),
                UiButtonVariant::Quiet,
            ),
            kit.button(
                "components.disabled",
                "ui.gallery.unavailable",
                command("gallery.unavailable"),
                UiButtonVariant::Standard,
            )
            .disabled(true),
        ],
    );
    let field = kit.text_input(
        "components.input",
        UiTextInput {
            placeholder_key: Some("ui.gallery.placeholder".into()),
            submit_command: Some("gallery.commit".into()),
            ..UiTextInput::new("components.value")
        },
    );
    let select = kit.select(
        "components.select",
        UiSelect::new(
            "components.mode",
            vec![
                UiSelectOption::new("fit", "ui.gallery.fit"),
                UiSelectOption::new("cover", "ui.gallery.cover"),
            ],
            0,
        ),
    );
    let flexible_field = |node: UiNode| {
        node.map_layout(|mut layout| {
            layout.grow = 1.0;
            layout.min_size[0] = m.control_height * 5.0;
            layout
        })
    };
    let fields = UiNode::row(
        "components.fields",
        [
            flexible_field(kit.property_field("components.name", "ui.gallery.name", field)),
            flexible_field(kit.property_field("components.mode", "ui.gallery.mode", select)),
        ],
    )
    .fit_content()
    .fill_width()
    .with_gap(m.spacing)
    .with_compact(UiCompactMode::Stack);
    let media = |id: &str, caption: &str, source: String, tint: Option<[u8; 4]>| {
        let image = UiImageView::new(source)
            .alt(caption)
            .shape(UiImageViewShape::Rounded(m.corner_radius));
        let image = if let Some(tint) = tint {
            image.tint(tint)
        } else {
            image
        };
        UiNode::column(
            id,
            [
                image.build(
                    format!("{id}.image"),
                    UiLayout::default()
                        .fill_width()
                        .with_aspect_ratio(16.0 / 9.0),
                ),
                UiNode::new(format!("{id}.caption"), UiNodeKind::Label)
                    .with_text_key(caption)
                    .with_text_style(UiTextStyle::label(t.text_muted))
                    .with_layout(UiLayout::fit_content().fill_width()),
            ],
        )
        .fit_content()
        .fill_width()
        .with_gap(m.spacing)
        .map_layout(|mut layout| {
            layout.grow = 1.0;
            layout.min_size[0] = m.control_height * 6.0;
            layout
        })
    };
    let previews = UiNode::row(
        "components.previews",
        [
            media(
                "components.preview",
                "ui.gallery.preview",
                super::images::builtin_icon_key(UiIconId::Assets),
                Some(t.text),
            ),
            media(
                "components.missing",
                "ui.gallery.missing",
                "assets://ui/gallery-unavailable.png".into(),
                None,
            ),
        ],
    )
    .fit_content()
    .fill_width()
    .with_gap(m.spacing * 2.0)
    .with_compact(UiCompactMode::Stack);
    let root = UiNode::scroll_view("components.root", UiScrollAxis::Vertical)
        .with_layout(
            UiLayout::fill(UiFlow::Column)
                .gap(m.spacing * 2.0)
                .padding(UiSpacing::same(m.spacing * 2.0)),
        )
        .with_style(UiStyle {
            fill: t.surface,
            text: t.text,
            ..UiStyle::transparent()
        })
        .with_children([title, description, actions, fields, previews]);
    UiSurface::new("rafui.components", palette, root).with_style_sheet(kit.style_sheet())
}
