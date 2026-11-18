// Attach to a character root and bind move_x, move_z and jump in Project Settings.
fn on_start() {
    let player = owner();
    player.configure_character(0.45, 1.8);
}

fn on_fixed_update(dt) {
    let player = owner();
    player.character_move([input_axis("move_x"), 0.0, input_axis("move_z")]);
    if was_action_just_pressed("jump") {
        player.character_jump();
    }
}

