// Attach to an explicit Controller entity, not to every object.
// Author /World/Camera (Empty or Part); /World/Player is optional.
// Units: meters, seconds, radians. No automatic player or camera creation.
fn on_start() {
    if !is_client() { return; }
    let camera = entity("/World/Camera");
    if !is_valid(camera) { return; }
    camera.add_camera();
    camera.set_fov(65.0);
    camera.set_clip(0.1, 1000.0);
    activate_camera(camera);
}

fn on_update(dt) {
    if !is_client() { return; }
    // C switches to an explicitly authored alternative camera.
    if was_key_just_pressed("c") {
        let alternate = entity("/World/Overview");
        if is_valid(alternate) && has_camera(alternate) {
            activate_camera(alternate);
        }
    }
    let target = entity("/World/Player");
    if !is_valid(target) { return; }
    let speed = 3.0 * dt;
    if is_key_pressed("w") { target.move_by(0.0, 0.0, -speed); }
    if is_key_pressed("s") { target.move_by(0.0, 0.0, speed); }
    if is_key_pressed("a") { target.move_by(-speed, 0.0, 0.0); }
    if is_key_pressed("d") { target.move_by(speed, 0.0, 0.0); }
}

fn on_late_update(dt) {
    if !is_client() { return; }
    let camera = get_active_camera();
    let target = entity("/World/Player");
    if !is_valid(camera) || !is_valid(target) { return; }
    // Leave the alternate camera fixed. Missing/deleted target freezes the view.
    let follow_camera = entity("/World/Camera");
    if is_valid(follow_camera) && node_uuid(camera) == node_uuid(follow_camera) {
        camera.follow(target, [0.0, 2.0, 8.0], [0.0, 1.0, 0.0], 8.0, dt);
    }
}
