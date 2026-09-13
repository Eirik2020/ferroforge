use std::path::{Path, PathBuf};

use ferroforge_renderer::{
    composition::{
        ConfigurationBinding, MonotonicProfile, MonotonicSource, ResourceBinding, SpawnBinding,
        StandaloneComposition, TaskSelection, validate_composition,
    },
    source::{DefinitionId, ModuleId, TaskSources, discover_task_package},
};

fn fixture_manifest() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sw/Cargo.toml")
}

fn id(sources: &TaskSources, module: &[&str], name: &str) -> DefinitionId {
    DefinitionId {
        module: ModuleId {
            crate_root: sources.root.crate_root.clone(),
            path: module.iter().map(|part| (*part).to_owned()).collect(),
        },
        name: name.to_owned(),
    }
}

fn binding(requirement: &str, resource: &str) -> ResourceBinding {
    ResourceBinding {
        requirement: requirement.to_owned(),
        resource: resource.to_owned(),
    }
}

fn valid(sources: &TaskSources) -> StandaloneComposition {
    StandaloneComposition {
        tasks: vec![
            TaskSelection {
                instance: "status".to_owned(),
                definition: id(sources, &["indicators"], "blink"),
                priority: 1,
                local: vec![
                    binding("led", "status_led"),
                    binding("count", "blink_count"),
                ],
                shared: vec![binding("enabled", "blink_enabled")],
                configuration: vec![ConfigurationBinding {
                    name: "period_ms".to_owned(),
                    rust_type: "u32".to_owned(),
                    value: "500".to_owned(),
                }],
                spawn: vec![SpawnBinding {
                    alias: "report".to_owned(),
                    target: "telemetry".to_owned(),
                }],
            },
            TaskSelection {
                instance: "telemetry".to_owned(),
                definition: id(sources, &["indicators"], "report"),
                priority: 1,
                local: vec![],
                shared: vec![],
                configuration: vec![],
                spawn: vec![],
            },
        ],
        monotonic: Some(MonotonicProfile::initial_systick()),
    }
}

fn sources() -> TaskSources {
    discover_task_package(&fixture_manifest()).unwrap().sources
}

#[test]
fn validates_complete_resource_configuration_spawn_and_profile_bindings() {
    let sources = sources();
    let validated = validate_composition(&sources, &valid(&sources)).unwrap();
    assert_eq!(validated.tasks.len(), 2);
    assert_eq!(validated.tasks[0].source.name, "status");
    assert_eq!(validated.tasks[0].local[0].resource, "status_led");
    assert_eq!(validated.tasks[0].configuration[0].value, "500");
    assert_eq!(
        validated.monotonic,
        Some(MonotonicProfile::initial_systick())
    );
}

#[test]
fn rejects_missing_unknown_duplicate_and_wrong_category_resources() {
    let sources = sources();
    for (change, expected) in [
        ("missing", "missing local resource bindings: count"),
        ("unknown", "no local resource requirement `other`"),
        (
            "duplicate",
            "binds system resource `status_led` more than once",
        ),
        (
            "category",
            "binds `enabled` as local, but it is declared shared",
        ),
    ] {
        let mut composition = valid(&sources);
        match change {
            "missing" => {
                composition.tasks[0].local.pop();
            }
            "unknown" => composition.tasks[0].local[0].requirement = "other".to_owned(),
            "duplicate" => composition.tasks[0].local[1].resource = "status_led".to_owned(),
            "category" => composition.tasks[0].local[0].requirement = "enabled".to_owned(),
            _ => unreachable!(),
        }
        let error = validate_composition(&sources, &composition)
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "{change}: {error}");
    }
}

#[test]
fn shares_shared_resources_but_rejects_reused_local_resources_across_instances() {
    let sources = sources();
    let mut composition = valid(&sources);
    let mut alarm = composition.tasks[0].clone();
    alarm.instance = "alarm".to_owned();
    alarm.local[0].resource = "alarm_led".to_owned();
    alarm.local[1].resource = "alarm_count".to_owned();
    alarm.configuration[0].value = "100".to_owned();
    composition.tasks.insert(1, alarm);
    validate_composition(&sources, &composition).unwrap();

    composition.tasks[1].local[0].resource = "status_led".to_owned();
    let error = validate_composition(&sources, &composition)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("local system resource `status_led` is bound to more than one"),
        "{error}"
    );
}

#[test]
fn rejects_incomplete_or_invalid_configuration_bindings() {
    let sources = sources();
    for (change, expected) in [
        ("missing", "missing configuration bindings: period_ms"),
        ("unknown", "has no configuration `other`"),
        ("type", "must use declared type `u32`"),
        ("value", "invalid value expression"),
    ] {
        let mut composition = valid(&sources);
        match change {
            "missing" => composition.tasks[0].configuration.clear(),
            "unknown" => composition.tasks[0].configuration[0].name = "other".to_owned(),
            "type" => composition.tasks[0].configuration[0].rust_type = "u64".to_owned(),
            "value" => composition.tasks[0].configuration[0].value = "let".to_owned(),
            _ => unreachable!(),
        }
        let error = validate_composition(&sources, &composition)
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "{change}: {error}");
    }
}

#[test]
fn rejects_incomplete_unknown_or_argument_count_mismatched_spawn_bindings() {
    let sources = sources();
    for (change, expected) in [
        ("missing", "missing spawn bindings: report"),
        ("alias", "has no spawn alias `other`"),
        ("target", "spawn target `absent` is not selected"),
        (
            "arguments",
            "declares 1 inputs, but target `empty` accepts 0",
        ),
    ] {
        let mut composition = valid(&sources);
        match change {
            "missing" => composition.tasks[0].spawn.clear(),
            "alias" => composition.tasks[0].spawn[0].alias = "other".to_owned(),
            "target" => composition.tasks[0].spawn[0].target = "absent".to_owned(),
            "arguments" => {
                composition.tasks.push(TaskSelection {
                    instance: "empty".to_owned(),
                    definition: id(&sources, &[], "root_task"),
                    priority: 1,
                    local: vec![],
                    shared: vec![],
                    configuration: vec![],
                    spawn: vec![],
                });
                composition.tasks[0].spawn[0].target = "empty".to_owned();
            }
            _ => unreachable!(),
        }
        let error = validate_composition(&sources, &composition)
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "{change}: {error}");
    }
}

#[test]
fn requires_the_supported_initial_monotonic_profile() {
    let sources = sources();
    let mut missing = valid(&sources);
    missing.monotonic = None;
    assert!(
        validate_composition(&sources, &missing)
            .unwrap_err()
            .to_string()
            .contains("requires a monotonic profile")
    );

    for profile in [
        MonotonicProfile {
            source: MonotonicSource::SysTick,
            tick_hz: 100,
            counter_bits: 32,
        },
        MonotonicProfile {
            source: MonotonicSource::Timer("TIM5".to_owned()),
            tick_hz: 1_000,
            counter_bits: 32,
        },
        MonotonicProfile {
            source: MonotonicSource::SysTick,
            tick_hz: 1_000,
            counter_bits: 64,
        },
    ] {
        let mut composition = valid(&sources);
        composition.monotonic = Some(profile);
        let error = validate_composition(&sources, &composition)
            .unwrap_err()
            .to_string();
        assert!(error.contains("unsupported monotonic profile"), "{error}");
    }
}
