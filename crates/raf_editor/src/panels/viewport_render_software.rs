use super::*;

impl ViewportPanel {
    pub(super) fn should_use_software_raster(&self, edit_target: Option<SceneNodeId>) -> bool {
        self.render_cfg.depth_accurate
            && edit_target.is_none()
            && self.render_style == RenderStyle::Solid
            && !self.solid_xray_mode
    }

    pub(super) fn solid_face_draw_style(
        &self,
        color: &NodeColor,
        is_selected: bool,
        brightness: f32,
    ) -> ([u8; 4], bool, [u8; 4], f32) {
        let shaded = self.solid_face_color(color, is_selected, brightness);
        let wire_color = if self.solid_show_surface_edges {
            self.solid_edge_color(is_selected)
        } else {
            [0, 0, 0, 0]
        };

        (
            shaded,
            self.solid_show_surface_edges,
            wire_color,
            if is_selected { 1.8 } else { 1.0 },
        )
    }

    fn solid_face_color(&self, color: &NodeColor, is_selected: bool, brightness: f32) -> [u8; 4] {
        let alpha = if self.solid_xray_mode { 120 } else { 255 };
        let tonality = if self.solid_face_tonality {
            brightness
        } else {
            1.0
        };

        if is_selected {
            depth_sort::shade_color(
                brighten_channel(color.r, 18),
                brighten_channel(color.g, 18),
                brighten_channel(color.b, 18),
                alpha,
                tonality,
            )
        } else {
            depth_sort::shade_color(color.r, color.g, color.b, alpha, tonality)
        }
    }

    fn solid_edge_color(&self, is_selected: bool) -> [u8; 4] {
        if is_selected {
            [theme::ACCENT.r(), theme::ACCENT.g(), theme::ACCENT.b(), 235]
        } else {
            [26, 26, 30, 150]
        }
    }

    pub(super) fn draw_3d_software_scene(
        &mut self,
        ctx: &egui::Context,
        painter: &egui::Painter,
        rect: Rect,
        scene: &SceneGraph,
        view_proj: &Mat4,
        light_dir: Vec3,
    ) {
        let scale = self.render_cfg.depth_resolution_scale.clamp(0.25, 1.0);
        let fb_width = (rect.width() * scale).round().max(1.0) as u32;
        let fb_height = (rect.height() * scale).round().max(1.0) as u32;
        let mut framebuffer = self
            .sw_framebuffer
            .take()
            .unwrap_or_else(|| SoftwareFramebuffer::new(fb_width, fb_height));

        framebuffer.resize(fb_width, fb_height);
        framebuffer.clear(240, 240, 242, 255);

        let raster_w = fb_width as f32;
        let raster_h = fb_height as f32;
        let mut tri_count = 0;

        for (id, node) in scene.iter() {
            if !node.visible || node.primitive == Primitive::Empty {
                continue;
            }

            let model = scene.world_matrix(id);
            let is_selected = self.selected.contains(&id);
            let distance_to_camera = (self.camera.position - node.position).length();
            let (sphere_stacks, sphere_slices, cylinder_segments, _) =
                self.lod_profile(distance_to_camera);
            let editable_mesh = self.ensure_edit_mesh_for_render(
                id,
                node,
                sphere_stacks,
                sphere_slices,
                cylinder_segments,
                false,
            );

            if let Some(edit_mesh) = editable_mesh.as_ref() {
                for (triangle, normal) in edit_mesh.render_faces() {
                    let brightness = depth_sort::face_brightness(normal, light_dir, &model);
                    let color = self.solid_face_color(&node.color, is_selected, brightness);
                    let corners = [triangle[0], triangle[1], triangle[2], triangle[2]];

                    if let Some((screen, depth)) = project_quad_for_raster(
                        &corners,
                        &model,
                        view_proj,
                        raster_w,
                        raster_h,
                    ) {
                        rasterize_quad(&mut framebuffer, &screen, &depth, color);

                        if self.solid_show_surface_edges {
                            rasterize_selection_outline(
                                &mut framebuffer,
                                &screen,
                                &depth,
                                self.solid_edge_color(is_selected),
                                if is_selected { 2.0 } else { 1.0 },
                            );
                        }

                        if is_selected && self.render_cfg.selection_outline {
                            rasterize_selection_outline(
                                &mut framebuffer,
                                &screen,
                                &depth,
                                self.render_cfg.selection_outline_color,
                                2.25,
                            );
                        }

                        tri_count += 1;
                    }
                }

                continue;
            }

            let faces: Vec<([Vec3; 4], Vec3)> = match node.primitive {
                Primitive::Cube => mesh::cube_faces(),
                Primitive::Sphere => mesh::sphere_faces(sphere_stacks, sphere_slices),
                Primitive::Plane => mesh::plane_faces(),
                Primitive::Cylinder => mesh::cylinder_faces(cylinder_segments),
                Primitive::Sprite2D => mesh::plane_faces(),
                Primitive::Empty => continue,
            };

            for (corners, normal) in &faces {
                let brightness = depth_sort::face_brightness(*normal, light_dir, &model);
                let color = self.solid_face_color(&node.color, is_selected, brightness);

                if let Some((screen, depth)) = project_quad_for_raster(
                    corners,
                    &model,
                    view_proj,
                    raster_w,
                    raster_h,
                ) {
                    rasterize_quad(&mut framebuffer, &screen, &depth, color);

                    if self.solid_show_surface_edges {
                        rasterize_selection_outline(
                            &mut framebuffer,
                            &screen,
                            &depth,
                            self.solid_edge_color(is_selected),
                            if is_selected { 2.0 } else { 1.0 },
                        );
                    }

                    if is_selected && self.render_cfg.selection_outline {
                        rasterize_selection_outline(
                            &mut framebuffer,
                            &screen,
                            &depth,
                            self.render_cfg.selection_outline_color,
                            2.25,
                        );
                    }

                    tri_count += 2;
                }
            }
        }

        self.tri_count = tri_count;

        let color_image = egui::ColorImage::from_rgba_unmultiplied(
            framebuffer.dimensions(),
            framebuffer.pixels(),
        );
        self.sw_framebuffer = Some(framebuffer);

        if let Some(texture) = self.sw_texture.as_mut() {
            texture.set(color_image, egui::TextureOptions::LINEAR);
        } else {
            self.sw_texture = Some(ctx.load_texture(
                "viewport-software-raster",
                color_image,
                egui::TextureOptions::LINEAR,
            ));
        }

        if let Some(texture) = &self.sw_texture {
            painter.image(
                texture.id(),
                rect,
                Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
    }
}