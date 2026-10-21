// Optional authored asset: copy into your project and attach explicitly.
// F6 changes/saves a counter; F7 explicitly reloads it. No automatic attachment.
let waiting = true;
let ready = false;
fn on_start() { progress_load("manual"); }
fn on_update(dt) {
    if waiting {
        let report = progress_status();
        if report.status == "loaded" {
            progress_apply_loaded(); waiting = false; ready = true;
        } else if report.status == "missing" {
            progress_set("counter", 0); waiting = false; ready = true;
        } else if report.status == "error" || report.status == "schema_mismatch" {
            // Never overwrite unreadable/incompatible saves with empty data.
            print(report); waiting = false; ready = false;
        }
        return;
    }
    if !ready || progress_status().busy { return; }
    if was_key_just_pressed("f6") {
        progress_set("counter", progress_get("counter", 0) + 1);
        progress_save("manual");
    } else if was_key_just_pressed("f7") {
        progress_load("manual"); waiting = true;
    }
}
