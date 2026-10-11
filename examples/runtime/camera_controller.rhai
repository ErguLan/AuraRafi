// Optional user-authored behavior. Attach to a Controller, then assign
// Entity parameters: camera, target (optional), alternate (optional).
// No names, objects, camera components or characters are injected.
fn on_start() {
    if !is_client() { return; }
    let camera = entity_parameter("camera");
    if !is_valid(camera) || !has_camera(camera) { return; }
    camera.set_fov(parameter("fov", 65.0));
    activate_camera(camera);
}

fn on_update(dt) {
    if !is_client() { return; }
    // The user's parameter chooses the key and both view targets.
    if was_key_just_pressed(parameter("switch_key", "c")) {
        let primary = entity_parameter("camera");
        let alternate = entity_parameter("alternate");
        let active = get_active_camera();
        if is_valid(active) && is_valid(alternate) && node_uuid(active) == node_uuid(alternate) {
            if is_valid(primary) && has_camera(primary) { activate_camera(primary); }
            return;
        }
        if is_valid(alternate) && has_camera(alternate) {
            activate_camera(alternate);
        }
    }
}

fn on_late_update(dt) {
    if !is_client() { return; }
    let camera = get_active_camera();
    let target = entity_parameter("target");
    if !is_valid(camera) || !is_valid(target) { return; }
    // Leave the alternate camera fixed. Missing/deleted target freezes the view.
    let follow_camera = entity_parameter("camera");
    if is_valid(follow_camera) && node_uuid(camera) == node_uuid(follow_camera) {
        camera.follow(target,
            [parameter("offset_x", 0.0), parameter("offset_y", 2.0), parameter("offset_z", 8.0)],
            [0.0, parameter("aim_height", 1.0), 0.0], parameter("sharpness", 8.0), dt);
    }
}
