// Optional event-only example. Attach to any user-created receiver entity.
let me = owner();
let behavior = #{ enabled: false, events: ["signal.changed"] };
let signal = 0.0;
let received = 0;
fn on_start() { me.set_capability("signal_receiver", true); }
fn on_event(name, payload) {
    // The message contract and lamp rules belong to this example, not the engine.
    if type_of(payload) != "map" { return; }
    if !payload.contains("enabled") || !payload.contains("amount") { return; }
    signal = if payload.enabled { payload.amount } else { 0.0 };
    received += 1;
    me.set_property("signal", signal);
    me.set_property("received", received);
    if payload.enabled { me.set_color_rgb(255, 220, 80); }
    else { me.set_color_rgb(80, 85, 90); }
}
