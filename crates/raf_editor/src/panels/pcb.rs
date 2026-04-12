impl AuraRafiApp {
    /// Synchronize the 2D schematic into a 3D PCB layout representation.
    pub(crate) fn sync_pcb_layout(&mut self) {
        self.scene = raf_core::scene::SceneGraph::new();
        let root = self.scene.add_root("PCB Board");
        
        // Add a board base (FR4 Green material)
        let board_id = self.scene.add_child(root, "FR4 Substrate");
        if let Some(node) = self.scene.get_mut(board_id) {
            node.primitive = raf_core::Primitive::Cube;
            node.scale = glam::Vec3::new(10.0, 0.1, 10.0);
            node.color = raf_core::NodeColor::rgb(0, 76, 25); // dark green
            node.position = glam::Vec3::new(0.0, -0.05, 0.0);
        }

        // Add 3D footprints for each component
        for comp in &self.schematic_view.schematic.components {
            let id = self.scene.add_child(board_id, &comp.designator);
            if let Some(node) = self.scene.get_mut(id) {
                node.primitive = raf_core::Primitive::Cube;
                // Base footprint sizing - standard 0805 or DIP based on `footprint` field
                node.scale = if comp.footprint.contains("BAT") {
                    glam::Vec3::new(0.8, 0.4, 0.4)
                } else if comp.footprint.contains("DIP") {
                    glam::Vec3::new(0.6, 0.2, 0.4)
                } else if comp.footprint.contains("MAG") {
                    glam::Vec3::new(0.5, 0.5, 0.5)
                } else {
                    glam::Vec3::new(0.2, 0.1, 0.1) // 0805 smd approx
                };
                
                node.color = raf_core::NodeColor::rgb(38, 38, 38); // IC black
                // Schematic position is 2D (x,y), map it to XZ plane on board
                // Scale factor: divide schematic grid pixels by 50 to get 3D units
                node.position = glam::Vec3::new(comp.position.x / 50.0, 0.1, comp.position.y / 50.0);
                
                // Add pins as child copper pads
                for pin in &comp.pins {
                    let pin_id = self.scene.add_child(id, &pin.name);
                    if let Some(pin_node) = self.scene.get_mut(pin_id) {
                        pin_node.primitive = raf_core::Primitive::Cylinder;
                        pin_node.scale = glam::Vec3::new(0.04, 0.04, 0.04);
                        pin_node.color = raf_core::NodeColor::rgb(204, 178, 51); // Gold/Copper
                        pin_node.position = glam::Vec3::new(pin.offset.x / 1.5, 0.0, pin.offset.y / 1.5);
                    }
                }
            }
        }
        
        let light = self.scene.add_child(root, "PCB Light");
        if let Some(node) = self.scene.get_mut(light) {
            node.position = glam::Vec3::new(5.0, 5.0, 5.0);
            node.color = raf_core::NodeColor::rgb(255, 255, 255);
        }
    }
}
