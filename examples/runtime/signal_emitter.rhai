// Optional example. Attach to a user-created entity; assign an Entity parameter named receiver.
// Never creates objects; not installed or executed automatically.
let me = owner();
let behavior = #{ update_interval: 0.25, enabled: true, events: [] };
let amount = parameter("signal_amount", 1.0);
let enabled = false;
fn on_start() { me.set_capability("signal_source", true); }
fn on_update(dt) {
    let receiver = entity_parameter("receiver");
    if !is_valid(receiver) { return; }
    enabled = !enabled;
    send_event(receiver, "signal.changed", #{ enabled: enabled, amount: amount, source_uuid: node_uuid(me) });
}
