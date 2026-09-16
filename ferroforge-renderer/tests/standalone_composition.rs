use std::path::{Path, PathBuf};

use ferroforge_renderer::{
    composition::{
        ConfigurationBinding, MonotonicProfile, MonotonicSource, ResourceBinding, SpawnBinding,
        StandaloneComposition, TaskKind, TaskSelection, validate_composition,
        validate_composition_across,
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
                interrupt: None,
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
                interrupt: None,
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

/// `interrupts::on_tick` is authored as a synchronous `fn`, so the contract reads it
/// as a hardware task and composition must supply its interrupt.
fn hardware(sources: &TaskSources, instance: &str, interrupt: Option<&str>) -> TaskSelection {
    TaskSelection {
        instance: instance.to_owned(),
        definition: id(sources, &["interrupts"], "on_tick"),
        priority: 2,
        interrupt: interrupt.map(str::to_owned),
        local: vec![binding("ticks", "tick_count")],
        shared: vec![],
        configuration: vec![],
        spawn: vec![],
    }
}

/// A firmware draws portable software tasks from one crate and, eventually,
/// HAL-specific hardware tasks from another. Definitions carry the crate root
/// they came from, so one composition can span packages.
#[test]
fn validates_a_composition_spanning_two_packages() {
    let fixture = discover_task_package(&fixture_manifest()).unwrap();
    let blinky = discover_task_package(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../tasks/blinky/Cargo.toml"),
    )
    .unwrap();
    let packages = [&fixture, &blinky];

    let composition = StandaloneComposition {
        tasks: vec![
            TaskSelection {
                instance: "status".to_owned(),
                definition: id(&blinky.sources, &[], "blink"),
                priority: 1,
                interrupt: None,
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
                definition: id(&blinky.sources, &[], "report"),
                priority: 1,
                interrupt: None,
                local: vec![],
                shared: vec![],
                configuration: vec![],
                spawn: vec![],
            },
            hardware(&fixture.sources, "tick", Some("TIM2")),
        ],
        monotonic: Some(MonotonicProfile::initial_systick()),
    };

    let validated = validate_composition_across(&packages, &composition).unwrap();
    assert_eq!(validated.tasks.len(), 3);
    assert_eq!(
        validated.tasks[0].source.package_key(),
        Some("ferroforge_task_blinky")
    );
    assert_eq!(
        validated.tasks[2].source.package_key(),
        Some("ferroforge_sw_task_fixture")
    );
    assert_eq!(validated.tasks[2].interrupt.as_deref(), Some("TIM2"));
}

#[test]
fn rejects_a_definition_from_a_package_the_composition_does_not_supply() {
    let fixture = discover_task_package(&fixture_manifest()).unwrap();
    let blinky = discover_task_package(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../tasks/blinky/Cargo.toml"),
    )
    .unwrap();

    // The composition names a blinky definition but supplies only the fixture.
    let composition = StandaloneComposition {
        tasks: vec![TaskSelection {
            instance: "status".to_owned(),
            definition: id(&blinky.sources, &[], "report"),
            priority: 1,
            interrupt: None,
            local: vec![],
            shared: vec![],
            configuration: vec![],
            spawn: vec![],
        }],
        monotonic: None,
    };

    let error = validate_composition_across(&[&fixture], &composition)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("from a package the composition does not supply"),
        "{error}"
    );
}

#[test]
fn accepts_a_hardware_task_with_an_interrupt_binding() {
    let sources = sources();
    let mut composition = valid(&sources);
    composition
        .tasks
        .push(hardware(&sources, "tick", Some("TIM2")));

    let validated = validate_composition(&sources, &composition).unwrap();
    let tick = validated.tasks.last().unwrap();
    assert_eq!(tick.interrupt.as_deref(), Some("TIM2"));
    assert_eq!(tick.kind(), TaskKind::Hardware);
    assert_eq!(validated.tasks[0].kind(), TaskKind::Software);
    assert_eq!(validated.tasks[0].interrupt, None);
}

#[test]
fn enforces_the_interrupt_binding_rule_for_each_task_kind() {
    let sources = sources();

    // G3: a hardware task requires a binding.
    let mut composition = valid(&sources);
    composition.tasks.push(hardware(&sources, "tick", None));
    let error = validate_composition(&sources, &composition)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("hardware task instance `tick` requires an interrupt binding"),
        "{error}"
    );

    // G3: a software task must not carry one.
    let mut composition = valid(&sources);
    composition.tasks[0].interrupt = Some("TIM2".to_owned());
    let error = validate_composition(&sources, &composition)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("software task instance `status` cannot bind interrupt `TIM2`"),
        "{error}"
    );

    // Nothing can spawn a hardware task; only its interrupt enters it.
    let mut composition = valid(&sources);
    composition
        .tasks
        .push(hardware(&sources, "tick", Some("TIM2")));
    composition.tasks[0].spawn[0].target = "tick".to_owned();
    let error = validate_composition(&sources, &composition)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("targets hardware task `tick`, which only its interrupt can enter"),
        "{error}"
    );

    // One interrupt cannot have two owners.
    let mut composition = valid(&sources);
    composition
        .tasks
        .push(hardware(&sources, "tick", Some("TIM2")));
    let mut second = hardware(&sources, "other_tick", Some("TIM2"));
    second.local[0].resource = "other_tick_count".to_owned();
    composition.tasks.push(second);
    let error = validate_composition(&sources, &composition)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("interrupt `TIM2` is bound by both `tick` and `other_tick`"),
        "{error}"
    );
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
                    interrupt: None,
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
