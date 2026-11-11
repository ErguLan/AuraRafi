use raf_ui::*;

#[test]
fn component_composition_preserves_flow_and_accessible_state_overrides() {
    let kit = UiComponents::for_palette(StudioUiPalette::PaperLight);
    let button = kit
        .button(
            "save",
            "app.save_menu",
            UiAction::Command {
                name: "project.save".into(),
            },
            UiButtonVariant::Standard,
        )
        .with_icon(UiIcon::new(UiIconId::File));
    let row = UiNode::row("actions", [button])
        .fit_content()
        .fill_width()
        .with_gap(kit.metrics.spacing)
        .with_padding(UiSpacing::same(kit.metrics.spacing));
    assert_eq!(row.layout.flow, UiFlow::Row);
    let button = &row.children[0];
    assert!(button.focusable);
    assert_eq!(
        button.accessibility_label_key.as_deref(),
        Some("app.save_menu")
    );
    let states = kit.style_sheet();
    let focused = states.resolve_with_state(
        button,
        UiVisualState {
            focused_id: Some("save"),
            ..Default::default()
        },
    );
    assert_eq!(focused.border, kit.tokens.focus);
    let invalid =
        states.resolve_with_state(&button.clone().invalid(true), UiVisualState::default());
    assert_eq!(invalid.border, kit.tokens.danger);
    let disabled = states.resolve(&button.clone().disabled(true));
    assert!(disabled.opacity < button.style.opacity);
    let override_fill = [30, 50, 70, 255];
    let custom = states.with_layer([UiStyleRule::new(
        UiStyleSelector::Id("save".into()),
        UiStylePatch {
            fill: Some(override_fill),
            ..Default::default()
        },
    )]);
    assert_eq!(
        custom
            .resolve_with_state(
                button,
                UiVisualState {
                    hovered_id: Some("save"),
                    ..Default::default()
                }
            )
            .fill,
        override_fill
    );
    let layout = UiLayout::default()
        .fill_width()
        .with_aspect_ratio(16.0 / 9.0);
    let saved = serde_json::to_value(&layout).unwrap();
    assert_eq!(serde_json::from_value::<UiLayout>(saved).unwrap(), layout);
    assert_eq!(layout.aspect_height(320.0), Some(180.0));
    assert_eq!(layout.fixed_height(40.0).aspect_height(320.0), None);
}

#[test]
fn drafts_preserve_caret_rejected_edits_and_cancel_to_latest_model_value() {
    let mut controls = UiControlState::default();
    let mut drafts = UiTextDraftBank::default();
    drafts.sync(
        "selection-a",
        [UiTextBinding::new("name", "Camera", 64)],
        &mut controls,
    );
    controls.set_text("name", "unfinished", 64);
    controls.set_cursor("name", 3, false);
    controls.set_ime_preedit("name", "x");
    drafts.sync(
        "selection-a",
        [UiTextBinding::new("name", "Renamed externally", 64)],
        &mut controls,
    );
    assert_eq!(controls.text("name"), "unfinished");
    assert_eq!(controls.text_edit("name").cursor, 3);
    assert_eq!(controls.ime_preedit("name"), "x");
    assert!(drafts.is_dirty("name", &controls));
    drafts.cancel("name", &mut controls);
    assert_eq!(controls.text("name"), "Renamed externally");
    assert_eq!(controls.ime_preedit("name"), "");
    drafts.sync(
        "selection-a",
        [UiTextBinding::new("name", "Confirmed", 64)],
        &mut controls,
    );
    assert_eq!(controls.text("name"), "Confirmed");
    controls.set_text("name", "another draft", 64);
    drafts.sync(
        "selection-b",
        [UiTextBinding::new("other", "New field", 64)],
        &mut controls,
    );
    assert!(!controls.has_text("name"));
    assert_eq!(controls.text("other"), "New field");
}
