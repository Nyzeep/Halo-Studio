use std::collections::BTreeSet;
use std::sync::Arc;

use bitfun_pi_rpc_adapter::{
    MemoryPiCredentialStore, MemoryPiRuntimeConfigurationRepository,
    PiRuntimeConfigurationRepository, PiRuntimeConfigurationService, StaticPiProviderCapabilities,
};
use bitfun_runtime_ports::{
    PiCredentialSecret, PiCredentialStorePort, PiProviderCapability, PiProviderCapabilityPort,
    PiProviderCapabilityRequest, PiProviderReadinessPort, PiRpcSessionMode, PiRuntimeConfiguration,
    PiStartupOptions, PiThinkingLevel, PortErrorKind,
};

fn configuration(model_id: &str, base_url: Option<&str>) -> PiRuntimeConfiguration {
    PiRuntimeConfiguration {
        provider_id: "openai".to_string(),
        base_url: base_url.map(str::to_string),
        model_id: model_id.to_string(),
        thinking_level: PiThinkingLevel::Medium,
        startup_options: PiStartupOptions::default(),
        credential_ref: "halo-pi-credential-v1-test".to_string(),
    }
}

fn capabilities() -> Arc<StaticPiProviderCapabilities> {
    Arc::new(StaticPiProviderCapabilities::new(vec![
        PiProviderCapability {
            provider_id: "openai".to_string(),
            model_id: "gpt-5".to_string(),
            api: "openai-completions".to_string(),
            accepts_base_url: true,
            supported_thinking_levels: vec![
                PiThinkingLevel::Off,
                PiThinkingLevel::Minimal,
                PiThinkingLevel::Low,
                PiThinkingLevel::Medium,
            ],
        },
    ]))
}

#[tokio::test]
async fn provider_capability_projection_contains_pi_api_and_model_metadata() {
    let configuration = configuration("gpt-5", Some("https://api.example.test/v1"));
    let capability = capabilities()
        .inspect(PiProviderCapabilityRequest {
            provider_id: configuration.provider_id.clone(),
            model_id: configuration.model_id.clone(),
            base_url: configuration.base_url.clone(),
        })
        .await
        .expect("capability fixture");

    let projection =
        bitfun_pi_rpc_adapter::pi_models_json_projection(&configuration, Some(&capability))
            .expect("Pi models projection");
    let provider = &projection["providers"]["openai"];
    assert_eq!(provider["api"], "openai-completions");
    assert_eq!(provider["models"][0]["id"], "gpt-5");
    assert_eq!(provider["models"][0]["reasoning"], true);
    for unverified in ["contextWindow", "maxTokens", "cost", "input"] {
        assert!(
            provider["models"][0].get(unverified).is_none(),
            "projection must not invent unverified model metadata: {unverified}"
        );
    }
    assert_eq!(provider["baseUrl"], "https://api.example.test/v1");
    assert!(!projection.to_string().contains("synthetic"));
}

#[tokio::test]
async fn configuration_crud_rollback_and_public_projection_keep_the_authority_narrow() {
    let repository = Arc::new(MemoryPiRuntimeConfigurationRepository::new());
    let service = PiRuntimeConfigurationService::new(repository.clone(), capabilities());
    let initial = configuration("gpt-5", Some("https://api.example.test/v1"));

    service
        .create(initial.clone())
        .await
        .expect("valid configuration is created");
    let updated = configuration("gpt-5", None);
    service
        .update(updated)
        .await
        .expect("existing configuration is updated");
    service.rollback().await.expect("update can be rolled back");

    let stored = repository
        .load()
        .await
        .expect("repository read")
        .expect("configuration remains after rollback");
    assert_eq!(stored, initial);
    let keys = serde_json::to_value(stored)
        .expect("configuration serializes")
        .as_object()
        .expect("configuration is an object")
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    assert_eq!(
        keys,
        BTreeSet::from([
            "baseUrl".to_string(),
            "credentialRef".to_string(),
            "modelId".to_string(),
            "providerId".to_string(),
            "startupOptions".to_string(),
            "thinkingLevel".to_string(),
        ])
    );

    let public = service.public_view().await.expect("public view");
    let public_json = serde_json::to_string(&public).expect("public view serializes");
    assert!(public_json.contains("<configured>"));
    assert!(!public_json.contains("https://api.example.test/v1"));
    assert!(!format!("{initial:?}").contains("api.example.test"));

    service.delete().await.expect("configuration deletes");
    assert!(repository.load().await.expect("repository read").is_none());
}

#[tokio::test]
async fn configuration_rollback_survives_service_reconstruction() {
    let repository = Arc::new(MemoryPiRuntimeConfigurationRepository::new());
    let service = PiRuntimeConfigurationService::new(repository.clone(), capabilities());
    let initial = configuration("gpt-5", Some("https://api.example.test/v1"));
    service
        .create(initial.clone())
        .await
        .expect("initial configuration");
    service
        .update(configuration("gpt-5", None))
        .await
        .expect("updated configuration");

    let reconstructed = PiRuntimeConfigurationService::new(repository.clone(), capabilities());
    reconstructed
        .rollback()
        .await
        .expect("persisted rollback is available after reconstruction");
    assert_eq!(
        repository
            .load()
            .await
            .expect("repository read")
            .expect("restored configuration"),
        initial
    );
}

#[tokio::test]
async fn json_configuration_repository_persists_rollback_across_service_reconstruction() {
    let root = tempfile::tempdir().expect("configuration repository root");
    let repository_path = root.path().join("halo-pi.json");
    let repository = Arc::new(
        bitfun_pi_rpc_adapter::JsonFilePiRuntimeConfigurationRepository::new(
            repository_path.clone(),
        ),
    );
    let service = PiRuntimeConfigurationService::new(repository.clone(), capabilities());
    let initial = configuration("gpt-5", Some("https://api.example.test/v1"));

    service
        .create(initial.clone())
        .await
        .expect("initial configuration");
    service
        .update(configuration("gpt-5", None))
        .await
        .expect("updated configuration");

    assert!(repository_path.exists());
    assert!(repository_path.with_extension("rollback.json").exists());

    let reconstructed = PiRuntimeConfigurationService::new(
        Arc::new(
            bitfun_pi_rpc_adapter::JsonFilePiRuntimeConfigurationRepository::new(repository_path),
        ),
        capabilities(),
    );
    reconstructed
        .rollback()
        .await
        .expect("persisted JSON rollback is available after reconstruction");
    assert_eq!(
        reconstructed.current().await.expect("configuration read"),
        Some(initial)
    );
}

#[tokio::test]
async fn configuration_rejects_invalid_base_url_startup_options_and_capabilities() {
    let repository = Arc::new(MemoryPiRuntimeConfigurationRepository::new());
    let service = PiRuntimeConfigurationService::new(repository, capabilities());

    for base_url in [
        Some("ftp://api.example.test"),
        Some("https://user:pass@api.example.test"),
        Some("https://api.example.test/v1?api_key=secret"),
        Some("https://api.example.test/v1#fragment"),
    ] {
        let error = service
            .create(configuration("gpt-5", base_url))
            .await
            .expect_err("unsafe base URL is rejected");
        assert_eq!(error.kind, PortErrorKind::InvalidRequest);
    }

    let mut options = PiStartupOptions::default();
    options.no_extensions = false;
    let mut invalid = configuration("gpt-5", None);
    invalid.startup_options = options;
    assert_eq!(
        service
            .create(invalid)
            .await
            .expect_err("discovery is disabled")
            .kind,
        PortErrorKind::InvalidRequest
    );

    let error = service
        .create(configuration("missing-model", None))
        .await
        .expect_err("unknown provider/model is rejected");
    assert_eq!(error.kind, PortErrorKind::InvalidRequest);

    let mut unsupported_thinking = configuration("gpt-5", None);
    unsupported_thinking.thinking_level = PiThinkingLevel::High;
    let error = service
        .create(unsupported_thinking)
        .await
        .expect_err("unsupported thinking level is rejected");
    assert_eq!(error.kind, PortErrorKind::InvalidRequest);
}

#[tokio::test]
async fn credential_store_returns_only_a_reference_and_fails_closed_on_mismatch_or_failure() {
    let store = MemoryPiCredentialStore::new();
    let reference = store
        .write(
            "openai",
            PiCredentialSecret::new("synthetic-credential-canary"),
        )
        .await
        .expect("credential write");
    assert!(reference.starts_with("halo-pi-credential-v1-"));
    assert!(!reference.contains("synthetic-credential-canary"));

    let value = store
        .read("openai", &reference)
        .await
        .expect("credential read")
        .into_string();
    assert_eq!(value, "synthetic-credential-canary");

    let mismatch = store
        .read("anthropic", &reference)
        .await
        .expect_err("provider mismatch fails closed");
    assert_eq!(mismatch.kind, PortErrorKind::PermissionDenied);

    let missing = store
        .read("openai", "halo-pi-credential-v1-missing")
        .await
        .expect_err("missing reference fails closed");
    assert_eq!(missing.kind, PortErrorKind::NotFound);

    store.set_read_failure(true);
    let failed = store
        .read("openai", &reference)
        .await
        .expect_err("credential store failure fails closed");
    assert_eq!(failed.kind, PortErrorKind::Backend);
}

#[test]
fn controlled_launch_projection_has_isolated_session_modes_and_no_api_key_flag() {
    let configuration = configuration("gpt-5", Some("https://api.example.test/v1"));
    let managed = bitfun_pi_rpc_adapter::pi_rpc_arguments(
        &configuration,
        PiRpcSessionMode::Managed,
        "C:/halo/extension.ts",
        None,
    );
    assert!(managed.windows(2).any(|pair| pair == ["--mode", "rpc"]));
    assert!(managed.contains(&"--no-session".to_string()));
    assert!(!managed.contains(&"--session-dir".to_string()));
    assert!(!managed.iter().any(|argument| argument == "--api-key"));
    assert!(!managed
        .iter()
        .any(|argument| argument.contains("api.example.test")));

    let standard = bitfun_pi_rpc_adapter::pi_rpc_arguments(
        &configuration,
        PiRpcSessionMode::Standard,
        "C:/halo/extension.ts",
        Some("C:/halo/task-session"),
    );
    assert!(standard
        .windows(2)
        .any(|pair| pair == ["--session-dir", "C:/halo/task-session"]));
    assert!(!standard.contains(&"--no-session".to_string()));
    assert!(!standard.iter().any(|argument| argument == "--api-key"));
}

#[tokio::test]
async fn capability_port_is_called_through_the_configuration_service_seam() {
    let repository = Arc::new(MemoryPiRuntimeConfigurationRepository::new());
    let capabilities = capabilities();
    let service = PiRuntimeConfigurationService::new(repository, capabilities.clone());
    service
        .create(configuration("gpt-5", None))
        .await
        .expect("capability-backed configuration");

    let request = PiProviderCapabilityRequest {
        provider_id: "openai".to_string(),
        model_id: "gpt-5".to_string(),
        base_url: None,
    };
    let capability = capabilities
        .inspect(request)
        .await
        .expect("capability seam");
    assert_eq!(capability.provider_id, "openai");
    assert_eq!(capability.model_id, "gpt-5");
}

#[tokio::test]
async fn configured_readiness_resolves_the_credential_reference_before_reporting_available() {
    let repository = Arc::new(MemoryPiRuntimeConfigurationRepository::new());
    let credentials = Arc::new(MemoryPiCredentialStore::new());
    let service = PiRuntimeConfigurationService::new_without_capabilities(repository)
        .with_credential_store(credentials.clone());

    service
        .create(configuration("gpt-5", None))
        .await
        .expect("configuration shape remains independently writable");
    assert_eq!(
        service
            .check()
            .await
            .expect_err("missing credential must not report readiness")
            .kind,
        PortErrorKind::NotFound
    );

    let credential_ref = credentials
        .write(
            "openai",
            PiCredentialSecret::new("synthetic-readiness-secret"),
        )
        .await
        .expect("readiness credential fixture");
    service
        .delete()
        .await
        .expect("remove missing configuration");
    let mut configured = configuration("gpt-5", None);
    configured.credential_ref = credential_ref.clone();
    service
        .create(configured)
        .await
        .expect("configuration with an opaque credential reference");
    assert_eq!(
        service
            .check()
            .await
            .expect("credential-backed readiness")
            .available,
        true
    );

    credentials
        .delete("openai", &credential_ref)
        .await
        .expect("delete readiness credential");
    assert_eq!(
        service
            .check()
            .await
            .expect_err("readiness must drop after credential deletion")
            .kind,
        PortErrorKind::NotFound
    );
}
