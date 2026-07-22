mod completion {
    use halo_core::completion::{complete_commands, default_commands};

    #[test]
    fn ranks_prefix_and_current_agent_above_plain_fuzzy_matches() {
        let commands = default_commands();
        let result = complete_commands(
            &commands,
            "/co",
            Some("codex-cli"),
            &["/review"],
            &["/codex"],
        );

        assert_eq!(result[0].name, "/codex");
        assert!(result[0].score > result[1].score);
    }

    #[test]
    fn suggests_arguments_after_command_name() {
        let commands = default_commands();
        let result = complete_commands(&commands, "/codex --", Some("codex-cli"), &[], &[]);
        let names: Vec<_> = result.iter().map(|item| item.name.as_str()).collect();

        assert!(names.contains(&"--continue"));
        assert!(names.contains(&"--model"));
        assert!(names.contains(&"--sandbox"));
    }
}
