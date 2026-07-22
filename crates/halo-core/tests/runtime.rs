mod runtime {
    use halo_core::runtime::FakeAgentRuntime;

    #[test]
    fn fake_runtime_emits_ordered_events_for_each_run() {
        let runtime = FakeAgentRuntime::default();

        for agent_count in [4, 16] {
            let events = runtime.run_scripted_agents(agent_count);

            assert_eq!(events.len(), agent_count * 7);

            for run_index in 0..agent_count {
                let run_id = format!("run-{}", run_index + 1);
                let seq: Vec<u64> = events
                    .iter()
                    .filter(|event| event.run_id == run_id)
                    .map(|event| event.seq)
                    .collect();

                assert_eq!(seq, vec![1, 2, 3, 4, 5, 6, 7]);
            }
        }
    }

    #[test]
    fn fake_runtime_emits_the_scripted_sequence_for_32_agents() {
        let runtime = FakeAgentRuntime::default();
        let events = runtime.run_scripted_agents(32);
        let run_one_kinds: Vec<_> = events
            .iter()
            .filter(|event| event.run_id == "run-1")
            .map(|event| event.kind.as_str())
            .collect();

        assert_eq!(events.len(), 32 * 7);
        assert_eq!(
            run_one_kinds,
            vec![
                "run.state",
                "message.created",
                "thinking.delta",
                "tool.started",
                "tool.completed",
                "message.completed",
                "token.updated",
            ]
        );
        assert_eq!(
            events.last().map(|event| event.kind.as_str()),
            Some("token.updated")
        );
    }
}
