mod scheduler {
    use halo_core::scheduler::RunScheduler;

    #[test]
    fn scheduler_limits_global_and_agent_concurrency() {
        let mut scheduler = RunScheduler::new(4, 2);

        for id in 0..8 {
            scheduler.enqueue("codex-cli", format!("task-{id}"));
        }

        for id in 8..16 {
            scheduler.enqueue("claude-code", format!("task-{id}"));
        }

        let started = scheduler.start_ready();
        let codex_count = started
            .iter()
            .filter(|run| run.agent_id == "codex-cli")
            .count();
        let claude_count = started
            .iter()
            .filter(|run| run.agent_id == "claude-code")
            .count();

        assert_eq!(started.len(), 4);
        assert_eq!(codex_count, 2);
        assert_eq!(claude_count, 2);
        assert_eq!(scheduler.running_count(), 4);
    }

    #[test]
    fn scheduler_finishes_running_run_and_starts_next_queued_run() {
        let mut scheduler = RunScheduler::new(4, 1);
        scheduler.enqueue("codex-cli", "task-0");
        scheduler.enqueue("codex-cli", "task-1");

        let first = scheduler.start_ready();

        assert_eq!(first.len(), 1);
        assert_eq!(first[0].task_id, "task-0");
        assert_eq!(scheduler.queued_count(), 1);

        let finished = scheduler.finish("task-0");
        let second = scheduler.start_ready();

        assert_eq!(finished.map(|run| run.task_id), Some("task-0".to_string()));
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].task_id, "task-1");
        assert_eq!(scheduler.running_count(), 1);
        assert_eq!(scheduler.queued_count(), 0);
    }
}
