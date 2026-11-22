use super::*;

#[test]
fn releasing_a_workload_rejects_stale_handles_and_returns_its_budget() {
    let mut device = Device::new(
        crate::backends::recorder::Recorder::default(),
        GraphicsMemoryBudget::potato(),
    )
    .unwrap();
    device.configure([640, 360], Output::Capture).unwrap();
    let fixture = crate::fixtures::scene(true, 3);
    let frame = crate::fixtures::camera([640, 360], 0.8);
    let before = device.ledger().resident_bytes;
    let handle = device.upload(&fixture).unwrap();
    assert_eq!(device.draw(handle, &frame).unwrap().draw_calls, 3);
    device.release(handle).unwrap();
    assert!(matches!(
        device.draw(handle, &frame),
        Err(Error::Invalid(_))
    ));
    assert_eq!(device.ledger().resident_bytes, before);
    assert_ne!(device.upload(&fixture).unwrap(), handle);
}

#[test]
fn rejects_invalid_geometry_before_admitting_any_bytes() {
    let mut device = Device::new(
        crate::backends::recorder::Recorder::default(),
        GraphicsMemoryBudget::potato(),
    )
    .unwrap();
    let mut fixture = crate::fixtures::scene(false, 3);
    fixture.batches[0].vertices.end += 1;
    assert!(matches!(device.upload(&fixture), Err(Error::Invalid(_))));
    assert_eq!(device.ledger().resident_bytes, 0);
}
