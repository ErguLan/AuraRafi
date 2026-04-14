impl AuraRafiApp {
    // -----------------------------------------------------------------------
    // v0.3.0: Undo/Redo 
    // -----------------------------------------------------------------------

    /// Push current scene state to undo stack.
    fn push_undo_snapshot(&mut self) {
        // Serialize scene to RON string (lightweight).
        if let Ok(data) = ron::ser::to_string(&self.scene) {
            self.undo_stack.push(data);
            // Cap at 50 to keep memory low.
            if self.undo_stack.len() > 50 {
                self.undo_stack.remove(0);
            }
            self.redo_stack.clear();
            self.scene_modified = true;
        }
    }

    fn do_undo(&mut self) {
        if let Some(snapshot) = self.undo_stack.pop() {
            // Save current to redo.
            if let Ok(current) = ron::ser::to_string(&self.scene) {
                self.redo_stack.push(current);
            }
            // Restore.
            if let Ok(restored) = ron::from_str::<SceneGraph>(&snapshot) {
                self.scene = restored;
                self.hierarchy.selected_node = None;
                self.viewport.selected.clear();
                let _lang = self.settings.language;
                let msg = t("app.undo", self.settings.language);
                self.last_action = msg.to_string();
                self.console.log(LogLevel::Info, &msg);
            }
        }
    }

    fn do_redo(&mut self) {
        if let Some(snapshot) = self.redo_stack.pop() {
            // Save current to undo.
            if let Ok(current) = ron::ser::to_string(&self.scene) {
                self.undo_stack.push(current);
            }
            // Restore.
            if let Ok(restored) = ron::from_str::<SceneGraph>(&snapshot) {
                self.scene = restored;
                self.hierarchy.selected_node = None;
                self.viewport.selected.clear();
                let _lang = self.settings.language;
                let msg = t("app.redo", self.settings.language);
                self.last_action = msg.to_string();
                self.console.log(LogLevel::Info, &msg);
            }
        }
    }

    // -----------------------------------------------------------------------
    // v0.3.0: Scene actions
    // -----------------------------------------------------------------------

    fn do_delete(&mut self) {
        let _lang = self.settings.language;
        if let Some(id) = self.hierarchy.selected_node {
            self.push_undo_snapshot();
            let name = self.scene.get(id).map(|n| n.name.clone()).unwrap_or_default();
            if self.scene.remove_node(id) {
                self.hierarchy.selected_node = None;
                self.viewport.selected.clear();
                let msg = format!("{} {}", t("app.deleted_msg", _lang), name);
                self.last_action = msg.clone();
                self.console.log(LogLevel::Info, &msg);
            }
        }
    }

    fn do_duplicate(&mut self) {
        let _lang = self.settings.language;
        if let Some(id) = self.hierarchy.selected_node {
            self.push_undo_snapshot();
            if let Some(new_id) = self.scene.duplicate_node(id) {
                self.hierarchy.selected_node = Some(new_id);
                self.viewport.selected = vec![new_id];
                let name = self.scene.get(new_id).map(|n| n.name.clone()).unwrap_or_default();
                let msg = format!("{} {}", t("app.duplicated_msg", _lang), name);
                self.last_action = msg.clone();
                self.console.log(LogLevel::Info, &msg);
            }
        }
    }

    fn do_select_all(&mut self) {
        let ids = self.scene.all_valid_ids();
        if let Some(first) = ids.first() {
            self.hierarchy.selected_node = Some(*first);
        }
        // Multi-select: select ALL entities.
        self.viewport.selected = ids.clone();
        let _lang = self.settings.language;
        let msg = format!("{} {}", ids.len(), t("app.entities_found_msg", _lang));
        self.last_action = msg.clone();
        self.console.log(LogLevel::Info, &msg);
    }

    fn do_save(&mut self) {
        let _lang = self.settings.language;
        if let Some(project) = &self.current_project {
            let _ = project.save();
            // Save scene alongside project.
            let scene_path = project.path.join("scene.ron");
            let _ = self.scene.save_ron(&scene_path);
            self.scene_modified = false;
            self.auto_save_elapsed = 0.0;
            let msg = t("app.project_saved", self.settings.language);
            self.last_action = msg.to_string();
            self.console.log(LogLevel::Info, &msg);
        }
    }

    // -----------------------------------------------------------------------
    // v0.3.0: Global shortcuts
    // -----------------------------------------------------------------------

    fn handle_global_shortcuts(&mut self, ctx: &egui::Context) {
        let action: Option<u8> = ctx.input(|i| {
            let ctrl = i.modifiers.ctrl || i.modifiers.mac_cmd;
            if ctrl && i.key_pressed(egui::Key::Z) { return Some(1); }
            if ctrl && i.key_pressed(egui::Key::Y) { return Some(2); }
            if ctrl && i.key_pressed(egui::Key::D) { return Some(3); }
            if ctrl && i.key_pressed(egui::Key::S) { return Some(4); }
            if ctrl && i.key_pressed(egui::Key::A) { return Some(5); }
            if i.key_pressed(egui::Key::Delete) { return Some(6); }
            None
        });
        match action {
            Some(1) => self.do_undo(),
            Some(2) => self.do_redo(),
            Some(3) => self.do_duplicate(),
            Some(4) => self.do_save(),
            Some(5) => self.do_select_all(),
            Some(6) => self.do_delete(),
            _ => {}
        }
    }
}
