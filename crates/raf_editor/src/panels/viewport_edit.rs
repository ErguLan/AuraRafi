use super::*;
use raf_render::editable::SelectionMode;

impl ViewportPanel {
    pub(super) fn toggle_edit_mode(&mut self, scene: &SceneGraph) {
        self.edit_mode = match self.edit_mode {
            EditMode::Object => EditMode::Vertex,
            EditMode::Vertex => EditMode::Object,
        };

        self.edit_drag_active = false;
        self.edit_last_pointer = [0.0; 2];

        if self.edit_mode == EditMode::Vertex {
            if let Some(sel_id) = self.selected.first().copied() {
                if let Some(node) = scene.get(sel_id) {
                    self.camera.target = node.position;
                    let max_dim = node.scale.x.max(node.scale.y).max(node.scale.z);
                    self.orbit_distance = (max_dim * 3.0).clamp(1.5, 30.0);
                    self.tool = ViewportTool::Select;
                    self.update_orbit_camera();
                }
            }
        }
    }

    pub(super) fn ensure_edit_mesh_for_render(
        &mut self,
        id: SceneNodeId,
        node: &SceneNode,
        sphere_stacks: usize,
        sphere_slices: usize,
        cylinder_segments: usize,
        create_if_missing: bool,
    ) -> Option<EditableMesh> {
        if let Some(mesh) = self.editable_meshes.get(&id) {
            return Some(mesh.clone());
        }
        if !create_if_missing {
            return None;
        }

        let mesh = match node.primitive {
            Primitive::Cube => EditableMesh::cube(),
            Primitive::Plane | Primitive::Sprite2D => EditableMesh::plane(),
            Primitive::Cylinder => EditableMesh::cylinder(cylinder_segments.max(8)),
            Primitive::Sphere => EditableMesh::sphere(sphere_stacks.max(4), sphere_slices.max(6)),
            Primitive::Empty => return None,
        };
        self.editable_meshes.insert(id, mesh.clone());
        Some(mesh)
    }

    pub(super) fn draw_editable_wireframe(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        model: &Mat4,
        view_proj: &Mat4,
        mesh: &EditableMesh,
        color: &NodeColor,
        is_selected: bool,
        vp_w: f32,
        vp_h: f32,
    ) {
        let wire_color = if is_selected {
            theme::ACCENT
        } else {
            Color32::from_rgba_premultiplied(color.r, color.g, color.b, 200)
        };
        let wire_width = if is_selected { 2.0 } else { 1.0 };

        for edge in mesh.wireframe_edges() {
            let transformed = [
                (*model * edge[0].extend(1.0)).truncate(),
                (*model * edge[1].extend(1.0)).truncate(),
            ];
            if let Some(screen_edge) = projection::project_edge(&[transformed[0], transformed[1]], view_proj, vp_w, vp_h) {
                let a = Pos2::new(rect.left() + screen_edge[0][0], rect.top() + screen_edge[0][1]);
                let b = Pos2::new(rect.left() + screen_edge[1][0], rect.top() + screen_edge[1][1]);
                painter.line_segment([a, b], Stroke::new(wire_width, wire_color));
            }
        }
    }

    pub(super) fn draw_edit_mode_overlay(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        model: &Mat4,
        view_proj: &Mat4,
        mesh: &EditableMesh,
        vp_w: f32,
        vp_h: f32,
    ) {
        let edge_indices = editable_edge_indices(mesh);
        let selected_edge = if mesh.selection.mode == SelectionMode::Edge && mesh.selection.vertices.len() == 2 {
            Some([
                mesh.selection.vertices[0].min(mesh.selection.vertices[1]),
                mesh.selection.vertices[0].max(mesh.selection.vertices[1]),
            ])
        } else {
            None
        };

        if mesh.selection.mode == SelectionMode::Face {
            for &face_index in &mesh.selection.faces {
                let Some(face) = mesh.faces.get(face_index) else {
                    continue;
                };
                let projected = face.indices.map(|vertex_index| {
                    let world = (*model * mesh.vertices[vertex_index].position.extend(1.0)).truncate();
                    projection::project_point(world, view_proj, vp_w, vp_h)
                });
                if projected.iter().all(|point| point.is_some()) {
                    let polygon = projected.map(|point| {
                        let point = point.unwrap();
                        Pos2::new(rect.left() + point[0], rect.top() + point[1])
                    });
                    painter.add(egui::Shape::convex_polygon(
                        polygon.to_vec(),
                        Color32::from_rgba_premultiplied(212, 119, 26, 45),
                        Stroke::new(1.5, Color32::WHITE),
                    ));
                }
            }
        }

        for edge in &edge_indices {
            let world = [
                (*model * mesh.vertices[edge[0]].position.extend(1.0)).truncate(),
                (*model * mesh.vertices[edge[1]].position.extend(1.0)).truncate(),
            ];
            if let Some(projected) = projection::project_edge(&world, view_proj, vp_w, vp_h) {
                let is_selected = selected_edge == Some(*edge);
                let color = if is_selected {
                    Color32::WHITE
                } else {
                    Color32::from_rgb(212, 119, 26)
                };
                let width = if is_selected { 2.5 } else { 1.4 };
                painter.line_segment(
                    [
                        Pos2::new(rect.left() + projected[0][0], rect.top() + projected[0][1]),
                        Pos2::new(rect.left() + projected[1][0], rect.top() + projected[1][1]),
                    ],
                    Stroke::new(width, color),
                );
            }
        }

        for (index, vertex) in mesh.vertices.iter().enumerate() {
            let world = (*model * vertex.position.extend(1.0)).truncate();
            let Some(screen) = projection::project_point(world, view_proj, vp_w, vp_h) else {
                continue;
            };
            let pos = Pos2::new(rect.left() + screen[0], rect.top() + screen[1]);
            let is_selected = mesh.selection.vertices.contains(&index);
            let fill = if is_selected {
                Color32::WHITE
            } else {
                Color32::from_rgb(212, 119, 26)
            };
            painter.circle_filled(pos, if is_selected { 4.8 } else { 3.6 }, fill);
            painter.circle_stroke(pos, if is_selected { 4.8 } else { 3.6 }, Stroke::new(1.0, Color32::from_rgba_premultiplied(20, 20, 20, 200)));
        }
    }

    pub(super) fn handle_edit_mode_input(
        &mut self,
        ui: &mut Ui,
        response: &egui::Response,
        rect: Rect,
        scene: &SceneGraph,
    ) -> bool {
        let Some(sel_id) = self.selected.first().copied() else {
            self.edit_drag_active = false;
            return false;
        };
        let Some(node) = scene.get(sel_id) else {
            self.edit_drag_active = false;
            return false;
        };

        let vp_w = rect.width();
        let vp_h = rect.height();
        let distance_to_camera = (self.camera.position - node.position).length();
        let (sphere_stacks, sphere_slices, cylinder_segments, _) = self.lod_profile(distance_to_camera);
        let _ = self.ensure_edit_mesh_for_render(
            sel_id,
            node,
            sphere_stacks,
            sphere_slices,
            cylinder_segments,
            true,
        );
        let model = scene.world_matrix(sel_id);
        let view_proj = self.camera.view_projection(vp_w, vp_h);
        let mut changed = false;

        if response.clicked() {
            if let Some(pointer) = response.interact_pointer_pos() {
                let click_local = [pointer.x - rect.left(), pointer.y - rect.top()];
                let shift = ui.input(|i| i.modifiers.shift);
                if let Some(mesh) = self.editable_meshes.get_mut(&sel_id) {
                    if let Some(vertex_idx) = pick_edit_vertex(mesh, &model, &view_proj, vp_w, vp_h, click_local) {
                        mesh.selection.mode = SelectionMode::Vertex;
                        if shift {
                            mesh.selection.toggle_vertex(vertex_idx);
                        } else {
                            mesh.selection.vertices.clear();
                            mesh.selection.vertices.push(vertex_idx);
                            mesh.selection.faces.clear();
                        }
                    } else if let Some(edge) = pick_edit_edge(mesh, &model, &view_proj, vp_w, vp_h, click_local) {
                        mesh.selection.mode = SelectionMode::Edge;
                        mesh.selection.faces.clear();
                        mesh.selection.vertices = vec![edge[0], edge[1]];
                    } else if let Some(face_idx) = pick_edit_face(mesh, &model, &view_proj, vp_w, vp_h, click_local) {
                        mesh.selection.mode = SelectionMode::Face;
                        if shift {
                            mesh.selection.toggle_face(face_idx);
                        } else {
                            mesh.selection.clear();
                            mesh.selection.faces.push(face_idx);
                        }
                        mesh.selection.vertices.clear();
                        for selected_face in mesh.selection.faces.clone() {
                            if let Some(face) = mesh.faces.get(selected_face).copied() {
                                mesh.selection.select_face_vertices(&face);
                            }
                        }
                    } else if !shift {
                        mesh.selection.clear();
                    }
                }
            }
        }

        if response.dragged_by(egui::PointerButton::Primary) && !ui.input(|i| i.modifiers.alt) {
            if let Some(pointer) = response.interact_pointer_pos() {
                if !self.edit_drag_active {
                    let has_selection = self
                        .editable_meshes
                        .get(&sel_id)
                        .map(|mesh| !mesh.selection.is_empty())
                        .unwrap_or(false);
                    if has_selection {
                        self.edit_drag_active = true;
                        self.edit_last_pointer = [pointer.x, pointer.y];
                    }
                } else {
                    let frame_delta = Vec2::new(
                        pointer.x - self.edit_last_pointer[0],
                        pointer.y - self.edit_last_pointer[1],
                    );
                    self.edit_last_pointer = [pointer.x, pointer.y];

                    if let Some(mesh) = self.editable_meshes.get_mut(&sel_id) {
                        match self.tool {
                            ViewportTool::Scale => {
                                let factor = (1.0
                                    + (frame_delta.x - frame_delta.y) * 0.01 * self.scale_sensitivity.max(0.1))
                                    .clamp(0.2, 4.0);
                                mesh.scale_selected(Vec3::splat(factor));
                                changed = true;
                            }
                            _ => {
                                let right = self.camera.view_matrix().row(0).truncate();
                                let up = self.camera.view_matrix().row(1).truncate();
                                let world_delta = (-right * frame_delta.x + up * frame_delta.y)
                                    * (self.orbit_distance * 0.0015 * self.move_sensitivity.max(0.1));
                                let local_delta = world_delta_to_local(node, world_delta);
                                mesh.move_selected(local_delta);
                                changed = true;
                            }
                        }
                    }
                }
            }
        }

        if response.drag_stopped() || !response.dragged_by(egui::PointerButton::Primary) {
            self.edit_drag_active = false;
            self.edit_last_pointer = [0.0; 2];
        }

        changed
    }
}

fn editable_edge_indices(mesh: &EditableMesh) -> Vec<[usize; 2]> {
    let mut edges: Vec<[usize; 2]> = Vec::new();
    for face in &mesh.faces {
        let pairs = [
            [face.indices[0], face.indices[1]],
            [face.indices[1], face.indices[2]],
            [face.indices[2], face.indices[0]],
        ];
        for mut pair in pairs {
            if pair[0] > pair[1] {
                pair.swap(0, 1);
            }
            if !edges.contains(&pair) {
                edges.push(pair);
            }
        }
    }
    edges
}

fn pick_edit_vertex(
    mesh: &EditableMesh,
    model: &Mat4,
    view_proj: &Mat4,
    vp_w: f32,
    vp_h: f32,
    click_local: [f32; 2],
) -> Option<usize> {
    let mut best: Option<(usize, f32)> = None;
    for (index, vertex) in mesh.vertices.iter().enumerate() {
        let world = (*model * vertex.position.extend(1.0)).truncate();
        let Some(screen) = projection::project_point(world, view_proj, vp_w, vp_h) else {
            continue;
        };
        let dx = screen[0] - click_local[0];
        let dy = screen[1] - click_local[1];
        let distance = (dx * dx + dy * dy).sqrt();
        if distance > 10.0 {
            continue;
        }
        let is_better = match best {
            None => true,
            Some((_, best_distance)) => distance < best_distance,
        };
        if is_better {
            best = Some((index, distance));
        }
    }
    best.map(|(index, _)| index)
}

fn pick_edit_edge(
    mesh: &EditableMesh,
    model: &Mat4,
    view_proj: &Mat4,
    vp_w: f32,
    vp_h: f32,
    click_local: [f32; 2],
) -> Option<[usize; 2]> {
    let mut best: Option<([usize; 2], f32)> = None;
    for edge in editable_edge_indices(mesh) {
        let world = [
            (*model * mesh.vertices[edge[0]].position.extend(1.0)).truncate(),
            (*model * mesh.vertices[edge[1]].position.extend(1.0)).truncate(),
        ];
        let Some(projected) = projection::project_edge(&world, view_proj, vp_w, vp_h) else {
            continue;
        };
        let distance = point_segment_distance(click_local, projected[0], projected[1]);
        if distance > 8.0 {
            continue;
        }
        let is_better = match best {
            None => true,
            Some((_, best_distance)) => distance < best_distance,
        };
        if is_better {
            best = Some((edge, distance));
        }
    }
    best.map(|(edge, _)| edge)
}

fn pick_edit_face(
    mesh: &EditableMesh,
    model: &Mat4,
    view_proj: &Mat4,
    vp_w: f32,
    vp_h: f32,
    click_local: [f32; 2],
) -> Option<usize> {
    let mut best: Option<(usize, f32)> = None;
    for (face_index, face) in mesh.faces.iter().enumerate() {
        let projected = face.indices.map(|vertex_index| {
            let world = (*model * mesh.vertices[vertex_index].position.extend(1.0)).truncate();
            projection::project_point(world, view_proj, vp_w, vp_h)
        });
        if projected.iter().any(|point| point.is_none()) {
            continue;
        }
        let triangle = projected.map(|point| point.unwrap());
        if !point_in_triangle(click_local, triangle[0], triangle[1], triangle[2]) {
            continue;
        }

        let centroid = [
            (triangle[0][0] + triangle[1][0] + triangle[2][0]) / 3.0,
            (triangle[0][1] + triangle[1][1] + triangle[2][1]) / 3.0,
        ];
        let dx = click_local[0] - centroid[0];
        let dy = click_local[1] - centroid[1];
        let distance = (dx * dx + dy * dy).sqrt();
        let is_better = match best {
            None => true,
            Some((_, best_distance)) => distance < best_distance,
        };
        if is_better {
            best = Some((face_index, distance));
        }
    }
    best.map(|(face_index, _)| face_index)
}

fn point_segment_distance(point: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let len_sq = dx * dx + dy * dy;
    if len_sq <= 0.0001 {
        let ex = point[0] - a[0];
        let ey = point[1] - a[1];
        return (ex * ex + ey * ey).sqrt();
    }
    let t = (((point[0] - a[0]) * dx + (point[1] - a[1]) * dy) / len_sq).clamp(0.0, 1.0);
    let closest = [a[0] + dx * t, a[1] + dy * t];
    let ex = point[0] - closest[0];
    let ey = point[1] - closest[1];
    (ex * ex + ey * ey).sqrt()
}

fn point_in_triangle(point: [f32; 2], a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> bool {
    let sign = |p1: [f32; 2], p2: [f32; 2], p3: [f32; 2]| {
        (p1[0] - p3[0]) * (p2[1] - p3[1]) - (p2[0] - p3[0]) * (p1[1] - p3[1])
    };

    let d1 = sign(point, a, b);
    let d2 = sign(point, b, c);
    let d3 = sign(point, c, a);
    let has_neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let has_pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(has_neg && has_pos)
}

fn world_delta_to_local(node: &SceneNode, world_delta: Vec3) -> Vec3 {
    let inv_rot = rotation_quat(node.rotation).inverse();
    let local_world = inv_rot * world_delta;
    Vec3::new(
        local_world.x / node.scale.x.max(0.01),
        local_world.y / node.scale.y.max(0.01),
        local_world.z / node.scale.z.max(0.01),
    )
}