use halo_core::{EventBus, RunSnapshot, RunState, RuntimeEvent};

#[test]
fn snapshot_keeps_latest_ring_buffer_events() {
    let mut snapshot = RunSnapshot::new("run-1", "codex-cli", 3);

    for seq in 1..=5 {
        snapshot.push_event(RuntimeEvent::new(
            "run-1",
            "codex-cli",
            seq,
            "message.delta",
            format!("event-{seq}"),
        ));
    }

    let seqs: Vec<u64> = snapshot.events().iter().map(|event| event.seq).collect();
    assert_eq!(seqs, vec![3, 4, 5]);
    assert_eq!(snapshot.last_seq(), 5);
}

#[test]
fn snapshot_tracks_run_state_from_state_events() {
    let mut snapshot = RunSnapshot::new("run-1", "codex-cli", 3);

    snapshot.push_event(RuntimeEvent::new(
        "run-1",
        "codex-cli",
        1,
        "run.state",
        "running",
    ));

    assert_eq!(snapshot.state(), &RunState::Running);
}

#[test]
fn snapshot_capacity_zero_still_tracks_last_sequence() {
    let mut snapshot = RunSnapshot::new("run-1", "codex-cli", 0);

    snapshot.push_event(RuntimeEvent::new(
        "run-1",
        "codex-cli",
        1,
        "message.delta",
        "hidden",
    ));

    assert!(snapshot.events().is_empty());
    assert_eq!(snapshot.last_seq(), 1);
}

#[test]
fn event_bus_rejects_out_of_order_events_for_run() {
    let mut bus = EventBus::new(8);

    bus.append(RuntimeEvent::new(
        "run-1",
        "codex-cli",
        1,
        "run.state",
        "running",
    ))
    .unwrap();

    let error = bus
        .append(RuntimeEvent::new(
            "run-1",
            "codex-cli",
            3,
            "message.delta",
            "gap",
        ))
        .unwrap_err();

    assert_eq!(error.to_string(), "expected seq 2 for run run-1, got 3");
}

#[test]
fn event_bus_keeps_sequences_independent_per_run() {
    let mut bus = EventBus::new(8);

    bus.append(RuntimeEvent::new(
        "run-1",
        "codex-cli",
        1,
        "message.delta",
        "one",
    ))
    .unwrap();
    bus.append(RuntimeEvent::new("run-2", "pi", 1, "message.delta", "two"))
        .unwrap();

    assert_eq!(bus.snapshot("run-1").unwrap().last_seq(), 1);
    assert_eq!(bus.snapshot("run-2").unwrap().last_seq(), 1);
}

#[test]
fn event_bus_does_not_create_snapshot_for_rejected_first_event() {
    let mut bus = EventBus::new(8);

    let error = bus
        .append(RuntimeEvent::new(
            "run-1",
            "codex-cli",
            2,
            "message.delta",
            "gap",
        ))
        .unwrap_err();

    assert_eq!(error.to_string(), "expected seq 1 for run run-1, got 2");
    assert!(bus.snapshot("run-1").is_none());
}

#[test]
fn event_bus_rejects_agent_id_changes_for_existing_run() {
    let mut bus = EventBus::new(8);

    bus.append(RuntimeEvent::new(
        "run-1",
        "codex-cli",
        1,
        "message.delta",
        "one",
    ))
    .unwrap();

    let error = bus
        .append(RuntimeEvent::new(
            "run-1",
            "pi",
            2,
            "message.delta",
            "wrong agent",
        ))
        .unwrap_err();

    assert_eq!(
        error.to_string(),
        "expected agent codex-cli for run run-1, got pi"
    );
    assert_eq!(bus.snapshot("run-1").unwrap().last_seq(), 1);
}
