// Attach to the authored camera entity (an Empty or a Part).
// World-axis translation + local yaw/pitch. No required character or rig.
fn on_start() {
    let camera = self_node();
    if is_client() && is_valid(camera) {
        camera.add_camera();
        activate_camera(camera);
    }
}
fn on_update(dt) {
    if !is_client() { return; }
    let camera = self_node();
    if !is_valid(camera) { return; }
    let speed = 4.0 * dt;
    if is_key_pressed("w") { camera.move_by(0.0, 0.0, -speed); }
    if is_key_pressed("s") { camera.move_by(0.0, 0.0, speed); }
    if is_key_pressed("a") { camera.move_by(-speed, 0.0, 0.0); }
    if is_key_pressed("d") { camera.move_by(speed, 0.0, 0.0); }
    if is_key_pressed("q") { camera.move_by(0.0, -speed, 0.0); }
    if is_key_pressed("e") { camera.move_by(0.0, speed, 0.0); }
    let r = camera.get_local_rotation();
    if is_key_pressed("left") { r[1] += dt; }
    if is_key_pressed("right") { r[1] -= dt; }
    if is_key_pressed("up") { r[0] += dt; }
    if is_key_pressed("down") { r[0] -= dt; }
    camera.set_local_rotation(r[0], r[1], r[2]);
}
