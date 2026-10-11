// Optional example: copy into the project and attach to an authored entity.
// Nothing in the engine loads this file automatically.
let speed = 4.0;       // m/s
let jump_impulse = 6.0; // kg*m/s; this is not a grounded character controller.

fn on_start() {
    let body = owner();
    body.set_aabb_collider([-0.5,-0.5,-0.5], [0.5,0.5,0.5]);
    body.configure_body("dynamic", true, false, 1.0, 0.0);
}
fn on_fixed_update(dt) {
    let body = owner();
    let velocity = body.get_velocity();
    velocity[0] = 0.0;
    velocity[2] = 0.0;
    if is_key_pressed("a") { velocity[0] -= speed; }
    if is_key_pressed("d") { velocity[0] += speed; }
    if is_key_pressed("w") { velocity[2] -= speed; }
    if is_key_pressed("s") { velocity[2] += speed; }
    body.set_velocity(velocity); // Preserve vertical velocity from gravity.
    if was_key_just_pressed("space") { body.apply_impulse([0.0,jump_impulse,0.0]); }
}
fn on_collision_enter(other) {
    if is_valid(other) { print("Collision with " + node_uuid(other)); }
}
fn on_trigger_enter(other) {
    if is_valid(other) { print("Entered trigger " + node_uuid(other)); }
}
fn on_trigger_exit(other) {
    // The other entity may have been removed. Never dereference without checking.
    if is_valid(other) { print("Exited trigger " + node_uuid(other)); }
}
