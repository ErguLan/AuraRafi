use super::*;


    fn find_node<'a>(node: &'a UiNode, id: &str) -> Option<&'a UiNode> {
        if node.id == id {
            return Some(node);
        }
        node.children.iter().find_map(|child| find_node(child, id))
    }

    fn collect_commands(node: &UiNode, out: &mut Vec<String>) {
        for binding in &node.event_handlers {
            if let raf_ui::UiAction::Command { name } = &binding.action {
                out.push(name.clone());
            }
        }
        for child in &node.children {
            collect_commands(child, out);
        }
    }

    /// True when a node answers both the double click and the secondary click
    /// with the quick-add palette, the way every engine node canvas does.
    fn opens_palette(node: &UiNode) -> bool {
        let double = node.event_handlers.iter().any(|binding| {
            binding.event == UiEventKind::DoubleClick
                && matches!(&binding.action, raf_ui::UiAction::Command { name } if name == "nodes.palette.open")
        });
        let secondary = node.event_handlers.iter().any(|binding| {
            binding.event == UiEventKind::ContextMenu
                && matches!(&binding.action, raf_ui::UiAction::OpenMenu { id } if id == "nodes.palette.open")
        });
        double && secondary
    }

    fn sample_graph() -> (NodeGraph, NodeId) {
        let mut graph = NodeGraph::new("Main Event Graph");
        let id = graph.add_node(Node::on_start());
        (graph, id)
    }

    fn build_sample_surface(graph: &NodeGraph, selected: Option<NodeId>) -> UiSurface {
        build_sample_view(graph, selected, [0.0, 0.0], [1200.0, 700.0]).0
    }

    fn build_sample_view(
        graph: &NodeGraph,
        selected: Option<NodeId>,
        pan: [f32; 2],
        viewport: [f32; 2],
    ) -> (UiSurface, NodesSurfaceHost) {
        let mut host = NodesSurfaceHost::default();
        host.open_palette_popup([400.0, 300.0], [240.0, 96.0]);
        let surface = build_nodes_surface_with_host(
            StudioUiPalette::IndustrialDark,
            graph,
            selected,
            1.0,
            pan,
            viewport,
            &host,
            false,
            false,
            None,
            raf_core::Language::English,
        );
        (surface, host)
    }

    fn card_rect(surface: &UiSurface, id: &str) -> UiRect {
        find_node(&surface.root, "nodes.canvas")
            .and_then(|canvas| {
                canvas
                    .children
                    .iter()
                    .find(|child| child.id == format!("nodes.card.{id}"))
            })
            .and_then(|card| card.layout.rect)
            .expect("card must render inside the canvas with an absolute rect")
    }

    #[test]
    fn canvas_is_viewport_sized_and_pans_the_graph() {
        let (graph, node_id) = sample_graph();
        let id = node_id.0.to_string();

        let (surface, _) = build_sample_view(&graph, None, [0.0, 0.0], [1200.0, 700.0]);
        let base = card_rect(&surface, &id);
        let (panned, _) = build_sample_view(&graph, None, [200.0, 100.0], [1200.0, 700.0]);
        let moved = card_rect(&panned, &id);

        // Panning moves the card without resizing it: the canvas never grows.
        assert_eq!(moved.width, base.width, "pan must not resize a card");
        assert_eq!(moved.height, base.height, "pan must not resize a card");
        assert_eq!(base.x - moved.x, 200.0);
        assert_eq!(base.y - moved.y, 100.0);

        let canvas = find_node(&surface.root, "nodes.canvas").expect("canvas must exist");
        let canvas_rect = canvas.layout.rect.expect("canvas is absolutely placed");
        assert_eq!(canvas_rect.width, 1200.0, "canvas fills the viewport");
        assert_eq!(
            canvas_rect.height,
            700.0 - NODES_TOOLBAR_HEIGHT,
            "canvas sits below the toolbar island"
        );
    }

    #[test]
    fn cards_keep_a_body_below_the_header() {
        let mut graph = NodeGraph::new("Main Event Graph");
        graph.add_node(Node::on_start());
        // A one-pin card must be clearly taller than its own header, otherwise
        // the body has nowhere to live and every node reads as a colored bar.
        let size = nodes_canvas::node_size(&graph.nodes[0], 1.0);
        assert!(
            size[1] >= nodes_canvas::NODE_HEADER_HEIGHT + nodes_canvas::NODE_PIN_ROW_HEIGHT,
            "card height {} leaves no body under a {} header",
            size[1],
            nodes_canvas::NODE_HEADER_HEIGHT
        );
    }

    #[test]
    fn quick_add_palette_spawns_nodes_at_the_cursor() {
        let (graph, _) = sample_graph();
        let (_, host) = build_sample_view(&graph, None, [0.0, 0.0], [1200.0, 700.0]);
        let popup = host.palette_popup().expect("popup is open");
        assert_eq!(popup.spawn, [240.0, 96.0]);

        let surface = build_nodes_palette_popup_surface(
            StudioUiPalette::IndustrialDark,
            &host,
            raf_core::Language::English,
        )
        .expect("open palette must render its own surface");
        let mut commands = Vec::new();
        collect_commands(&surface.root, &mut commands);
        assert!(
            commands
                .iter()
                .any(|command| command.starts_with("nodes.add.at:")
                    && command.ends_with(":240.0:96.0")),
            "palette rows must spawn at the pointer: {commands:?}"
        );
    }

    #[test]
    fn quick_add_palette_filters_out_unrelated_nodes() {
        let (_graph, _) = sample_graph();
        let mut host = NodesSurfaceHost::default();
        host.open_palette_popup([10.0, 10.0], [0.0, 0.0]);
        let all = build_nodes_palette_popup_surface(
            StudioUiPalette::IndustrialDark,
            &host,
            raf_core::Language::English,
        )
        .expect("palette surface");
        let mut all_commands = Vec::new();
        collect_commands(&all.root, &mut all_commands);

        host.set_palette_query("print");
        let filtered = build_nodes_palette_popup_surface(
            StudioUiPalette::IndustrialDark,
            &host,
            raf_core::Language::English,
        )
        .expect("filtered palette");
        let mut filtered_commands = Vec::new();
        collect_commands(&filtered.root, &mut filtered_commands);

        assert!(
            filtered_commands.len() < all_commands.len(),
            "a search must narrow the palette: {} vs {}",
            filtered_commands.len(),
            all_commands.len()
        );
    }

    #[test]
    fn empty_canvas_offers_double_click_and_secondary_click_palette() {
        // A populated graph answers on the canvas itself.
        let (graph, _) = sample_graph();
        let (surface, _) = build_sample_view(&graph, None, [0.0, 0.0], [1200.0, 700.0]);
        let canvas = find_node(&surface.root, "nodes.canvas").expect("canvas must exist");
        assert!(
            opens_palette(canvas),
            "canvas must open the quick-add palette"
        );

        // An empty graph has no canvas, so the empty state owns the gesture.
        let empty = NodeGraph::new("Main Event Graph");
        let (surface, _) = build_sample_view(&empty, None, [0.0, 0.0], [1200.0, 700.0]);
        let empty_state = find_node(&surface.root, "nodes.graph.empty").expect("empty state");
        assert!(
            opens_palette(empty_state),
            "empty state must open the quick-add palette"
        );
    }

    #[test]
    fn node_card_opens_its_own_context_menu_command() {
        let (graph, node_id) = sample_graph();
        let id = node_id.0.to_string();
        let surface = build_sample_surface(&graph, Some(node_id));
        let card = find_node(&surface.root, &format!("nodes.card.{id}"))
            .expect("selected graph must render its card");
        assert!(card.event_handlers.iter().any(|binding| {
            binding.event == UiEventKind::ContextMenu
                && matches!(
                    &binding.action,
                    raf_ui::UiAction::OpenMenu { id: menu_id }
                        if *menu_id == format!("nodes.context.{id}")
                )
        }));
    }

    #[test]
    fn empty_state_offers_the_on_start_shortcut() {
        let graph = NodeGraph::new("Main Event Graph");
        let surface = build_sample_surface(&graph, None);
        let cta = find_node(&surface.root, "nodes.graph.empty.cta")
            .expect("empty graph must render its call to action");
        assert!(cta.event_handlers.iter().any(|binding| {
            binding.event == UiEventKind::Click
                && matches!(
                    &binding.action,
                    raf_ui::UiAction::Command { name } if name == "nodes.add.on-start"
                )
        }));
    }

    #[test]
    fn context_menu_surface_lists_node_actions_and_handles_missing_nodes() {
        let (graph, node_id) = sample_graph();
        let id = node_id.0.to_string();
        let surface = build_nodes_context_menu_surface(
            StudioUiPalette::IndustrialDark,
            &graph,
            node_id,
            raf_core::Language::English,
        )
        .expect("existing node must produce a menu");

        let mut commands = Vec::new();
        collect_commands(&surface.root, &mut commands);
        assert!(commands.contains(&format!("nodes.duplicate.{id}")));
        assert!(commands.contains(&format!("nodes.disconnect_all.{id}")));
        assert!(commands.contains(&format!("nodes.delete.{id}")));

        let missing = NodeId::new();
        assert!(
            build_nodes_context_menu_surface(
                StudioUiPalette::IndustrialDark,
                &graph,
                missing,
                raf_core::Language::English,
            )
            .is_none(),
            "a deleted node must not render a stale menu"
        );
    }
