use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

use ferroforge_renderer::{
    composition::{
        ConfigurationBinding, MonotonicProfile, ResourceBinding, SpawnBinding,
        StandaloneComposition, TaskSelection, validate_composition,
    },
    source::{DefinitionId, ModuleId, TaskSources, discover_init_package, discover_task_package},
    transplant::{
        RticAppShell, RticAppTarget, SingleModuleRticShell, render_rtic_app,
        render_rtic_app_with_init, render_single_module_rtic_app,
    },
};

fn cargo() -> std::ffi::OsString {
    env::var_os("CARGO").unwrap_or_else(|| "cargo".into())
}

fn sw_manifest() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sw/Cargo.toml")
}

fn init_manifest() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/init/Cargo.toml")
}

fn rtic_fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rtic-layout")
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

fn selection(instance: &str, definition: DefinitionId, resource: &str) -> TaskSelection {
    TaskSelection {
        instance: instance.to_owned(),
        definition,
        priority: 1,
        local: Vec::new(),
        shared: vec![ResourceBinding {
            requirement: "sample".to_owned(),
            resource: resource.to_owned(),
        }],
        configuration: Vec::new(),
        spawn: Vec::new(),
    }
}

fn shell() -> SingleModuleRticShell {
    SingleModuleRticShell {
        crate_imports: vec!["use panic_probe as _;".to_owned()],
        device: "stm32f4xx_hal::pac".to_owned(),
        app_module: "app".to_owned(),
        dispatchers: vec!["USART1".to_owned()],
        shared: "struct Shared { state: __ferroforge_module_6_layout::Sample }".to_owned(),
        local: "struct Local {}".to_owned(),
        init: "fn init(_cx: init::Context) -> (Shared, Local) {
            alpha::spawn().unwrap();
            beta::spawn().unwrap();
            (Shared { state: __ferroforge_module_6_layout::Sample(0) }, Local {})
        }"
        .to_owned(),
    }
}

fn multi_module_shell() -> RticAppShell {
    RticAppShell {
        crate_imports: vec!["use panic_probe as _;".to_owned()],
        device: "stm32f4xx_hal::pac".to_owned(),
        app_module: "app".to_owned(),
        dispatchers: vec!["USART1".to_owned()],
        shared: "struct Shared {
            state: __ferroforge_module_6_layout::Sample,
            other_state: __ferroforge_module_12_layout_other::Sample,
        }"
        .to_owned(),
        local: "struct Local {}".to_owned(),
        init: "fn init(_cx: init::Context) -> (Shared, Local) {
            alpha::spawn().unwrap();
            gamma::spawn().unwrap();
            (Shared {
                state: __ferroforge_module_6_layout::Sample(0),
                other_state: __ferroforge_module_12_layout_other::Sample(false),
            }, Local {})
        }"
        .to_owned(),
    }
}

fn target() -> RticAppTarget {
    RticAppTarget {
        crate_imports: vec!["use panic_probe as _;".to_owned()],
        device: "stm32f4xx_hal::pac".to_owned(),
        app_module: "app".to_owned(),
        dispatchers: vec!["USART1".to_owned()],
    }
}

fn configuration_shell() -> SingleModuleRticShell {
    SingleModuleRticShell {
        crate_imports: vec!["use panic_probe as _;".to_owned()],
        device: "stm32f4xx_hal::pac".to_owned(),
        app_module: "app".to_owned(),
        dispatchers: vec!["USART1".to_owned()],
        shared: "struct Shared { state: __ferroforge_module_6_layout::Sample }".to_owned(),
        local: "struct Local {}".to_owned(),
        init: "fn init(_cx: init::Context) -> (Shared, Local) {
            cfg_a::spawn().unwrap();
            cfg_b::spawn().unwrap();
            (Shared { state: __ferroforge_module_6_layout::Sample(0) }, Local {})
        }"
        .to_owned(),
    }
}

fn spawn_shell() -> SingleModuleRticShell {
    SingleModuleRticShell {
        crate_imports: vec!["use panic_probe as _;".to_owned()],
        device: "stm32f4xx_hal::pac".to_owned(),
        app_module: "app".to_owned(),
        dispatchers: vec!["USART1".to_owned()],
        shared: "struct Shared {}".to_owned(),
        local: "struct Local {}".to_owned(),
        init: "fn init(_cx: init::Context) -> (Shared, Local) {
            caller::spawn().unwrap();
            (Shared {}, Local {})
        }"
        .to_owned(),
    }
}

fn spawn_composition(sources: &TaskSources, producer_definition: &str) -> StandaloneComposition {
    let mut spawn = vec![SpawnBinding {
        alias: "deliver".to_owned(),
        target: "sink".to_owned(),
    }];
    if producer_definition == "producer" {
        spawn.insert(
            0,
            SpawnBinding {
                alias: "wake".to_owned(),
                target: "idle".to_owned(),
            },
        );
        spawn.push(SpawnBinding {
            alias: "pair".to_owned(),
            target: "join".to_owned(),
        });
    }
    StandaloneComposition {
        tasks: vec![
            TaskSelection {
                instance: "caller".to_owned(),
                definition: id(sources, &["layout"], producer_definition),
                priority: 1,
                local: Vec::new(),
                shared: Vec::new(),
                configuration: Vec::new(),
                spawn,
            },
            TaskSelection {
                instance: "sink".to_owned(),
                definition: id(sources, &["layout"], "consumer"),
                priority: 1,
                local: Vec::new(),
                shared: Vec::new(),
                configuration: Vec::new(),
                spawn: Vec::new(),
            },
            TaskSelection {
                instance: "idle".to_owned(),
                definition: id(sources, &["layout"], "no_args"),
                priority: 1,
                local: Vec::new(),
                shared: Vec::new(),
                configuration: Vec::new(),
                spawn: Vec::new(),
            },
            TaskSelection {
                instance: "join".to_owned(),
                definition: id(sources, &["layout"], "pair_inputs"),
                priority: 1,
                local: Vec::new(),
                shared: Vec::new(),
                configuration: Vec::new(),
                spawn: Vec::new(),
            },
        ],
        monotonic: None,
    }
}

fn monotonic_shell() -> SingleModuleRticShell {
    SingleModuleRticShell {
        crate_imports: vec!["use panic_probe as _;".to_owned()],
        device: "stm32f4xx_hal::pac".to_owned(),
        app_module: "app".to_owned(),
        dispatchers: vec!["USART1".to_owned()],
        shared: "struct Shared {}".to_owned(),
        local: "struct Local {}".to_owned(),
        init: "fn init(cx: init::Context) -> (Shared, Local) {
            crate::__FerroforgeMono::start(cx.core.SYST, 16_000_000);
            timed::spawn().unwrap();
            (Shared {}, Local {})
        }"
        .to_owned(),
    }
}

fn monotonic_composition(sources: &TaskSources, definition: &str) -> StandaloneComposition {
    let configuration = if definition == "periodic" {
        vec![ConfigurationBinding {
            name: "period_ms".to_owned(),
            rust_type: "u32".to_owned(),
            value: "5".to_owned(),
        }]
    } else {
        Vec::new()
    };
    StandaloneComposition {
        tasks: vec![TaskSelection {
            instance: "timed".to_owned(),
            definition: id(sources, &["layout"], definition),
            priority: 1,
            local: Vec::new(),
            shared: Vec::new(),
            configuration,
            spawn: Vec::new(),
        }],
        monotonic: Some(MonotonicProfile::initial_systick()),
    }
}

fn logging_shell() -> SingleModuleRticShell {
    SingleModuleRticShell {
        crate_imports: vec!["use panic_probe as _;".to_owned()],
        device: "stm32f4xx_hal::pac".to_owned(),
        app_module: "app".to_owned(),
        dispatchers: vec!["USART1".to_owned()],
        shared: "struct Shared { state: __ferroforge_module_6_layout::Sample }".to_owned(),
        local: "struct Local {}".to_owned(),
        init: "fn init(_cx: init::Context) -> (Shared, Local) {
            logger::spawn().unwrap();
            (Shared { state: __ferroforge_module_6_layout::Sample(9) }, Local {})
        }"
        .to_owned(),
    }
}

struct TempProject(PathBuf);

impl TempProject {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = env::temp_dir().join(format!(
            "ferroforge-standalone-transplant-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("src")).unwrap();
        fs::copy(rtic_fixture().join("Cargo.toml"), root.join("Cargo.toml")).unwrap();
        fs::copy(rtic_fixture().join("Cargo.lock"), root.join("Cargo.lock")).unwrap();
        Self(root)
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        // Only this test-owned unique directory is removed.
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn check_arm_source(source: &str) {
    let project = TempProject::new();
    fs::write(project.0.join("src/main.rs"), source).unwrap();
    let status = Command::new(cargo())
        .args([
            "check",
            "--bin",
            "ferroforge-rtic-layout-fixture",
            "--target",
            "thumbv7em-none-eabihf",
            "--locked",
            "--offline",
        ])
        .current_dir(&project.0)
        .status()
        .unwrap();
    assert!(status.success(), "renderer-owned RTIC layout must check");
}

#[test]
fn renderer_emits_a_compiling_single_module_rtic_layout() {
    let package = discover_task_package(&sw_manifest()).unwrap();
    let composition = StandaloneComposition {
        tasks: vec![
            selection("alpha", id(&package.sources, &["layout"], "first"), "state"),
            selection("beta", id(&package.sources, &["layout"], "first"), "state"),
        ],
        monotonic: None,
    };
    let validated = validate_composition(&package.sources, &composition).unwrap();
    let source = render_single_module_rtic_app(&package.sources, &validated, &shell()).unwrap();

    assert_eq!(source.matches("struct Sample").count(), 1);
    assert_eq!(source.matches("fn update").count(), 1);
    assert!(source.contains("pub struct Sample"));
    assert!(source.contains("pub fn update"));
    assert!(source.contains("async fn alpha"));
    assert!(source.contains("async fn beta"));
    assert!(!source.contains("async fn second"));
    assert!(source.contains("shared = [state]"), "{source}");
    assert!(source.contains("crate::__ferroforge_sources::__ferroforge_module_6_layout::update"));
    assert!(!source.contains("ferroforge::task"));

    check_arm_source(&source);
}

#[test]
fn renderer_isolates_colliding_support_from_multiple_modules() {
    let package = discover_task_package(&sw_manifest()).unwrap();
    let composition = StandaloneComposition {
        tasks: vec![
            selection("alpha", id(&package.sources, &["layout"], "first"), "state"),
            selection(
                "gamma",
                id(&package.sources, &["layout_other"], "other"),
                "other_state",
            ),
        ],
        monotonic: None,
    };
    let validated = validate_composition(&package.sources, &composition).unwrap();
    let error = render_single_module_rtic_app(&package.sources, &validated, &multi_module_shell())
        .unwrap_err()
        .to_string();
    assert!(error.contains("one logical source module"), "{error}");

    let source = render_rtic_app(&package.sources, &validated, &multi_module_shell()).unwrap();
    assert_eq!(source.matches("struct Sample").count(), 2);
    assert_eq!(source.matches("fn update").count(), 2);
    assert!(source.contains("mod __ferroforge_module_6_layout"));
    assert!(source.contains("mod __ferroforge_module_12_layout_other"));
    assert!(source.contains("shared = [state]"), "{source}");
    assert!(source.contains("crate::__ferroforge_sources::__ferroforge_module_6_layout::update"));
    assert!(source.contains("shared = [other_state]"), "{source}");
    assert!(
        source.contains("crate::__ferroforge_sources::__ferroforge_module_12_layout_other::update")
    );
    check_arm_source(&source);
}

#[test]
fn renderer_emits_per_instance_configuration_constants() {
    let package = discover_task_package(&sw_manifest()).unwrap();
    let mut cfg_a = selection(
        "cfg_a",
        id(&package.sources, &["layout"], "configured"),
        "state",
    );
    cfg_a.configuration.push(ConfigurationBinding {
        name: "step".to_owned(),
        rust_type: "u32".to_owned(),
        value: "2".to_owned(),
    });
    let mut cfg_b = selection(
        "cfg_b",
        id(&package.sources, &["layout"], "configured"),
        "state",
    );
    cfg_b.configuration.push(ConfigurationBinding {
        name: "step".to_owned(),
        rust_type: "u32".to_owned(),
        value: "3".to_owned(),
    });
    let composition = StandaloneComposition {
        tasks: vec![cfg_a, cfg_b],
        monotonic: None,
    };
    let validated = validate_composition(&package.sources, &composition).unwrap();
    let source =
        render_single_module_rtic_app(&package.sources, &validated, &configuration_shell())
            .unwrap();

    assert!(source.contains("mod __ferroforge_config"));
    assert!(source.contains("pub(super) mod cfg_a"));
    assert!(source.contains("pub(super) mod cfg_b"));
    assert!(source.contains("pub(crate) const STEP: u32 = 2;"));
    assert!(source.contains("pub(crate) const STEP: u32 = 3;"));
    assert_eq!(
        source.matches("__ferroforge_config::cfg_a::STEP").count(),
        1
    );
    assert_eq!(
        source.matches("__ferroforge_config::cfg_b::STEP").count(),
        1
    );
    assert!(!source.contains("CONFIG.STEP"));

    check_arm_source(&source);
}

#[test]
fn renderer_translates_spawn_aliases_to_selected_instances() {
    let package = discover_task_package(&sw_manifest()).unwrap();
    let composition = spawn_composition(&package.sources, "producer");
    let validated = validate_composition(&package.sources, &composition).unwrap();
    let source =
        render_single_module_rtic_app(&package.sources, &validated, &spawn_shell()).unwrap();

    assert!(source.contains("let _: Result<(), ()> = idle::spawn();"));
    assert!(source.contains("let _: Result<(), u32> = sink::spawn(7);"));
    assert!(source.contains("let _: Result<(), (u16, bool)> = join::spawn(11, true);"));
    assert!(!source.contains("cx.spawn.deliver"));
    assert!(!source.contains("cx.spawn.wake"));
    assert!(!source.contains("cx.spawn.pair"));
    assert!(source.contains("async fn sink(_cx: sink::Context, value: u32)"));

    check_arm_source(&source);
}

#[test]
fn renderer_binds_the_initial_systick_monotonic() {
    let package = discover_task_package(&sw_manifest()).unwrap();
    let composition = monotonic_composition(&package.sources, "periodic");
    let validated = validate_composition(&package.sources, &composition).unwrap();
    let source =
        render_single_module_rtic_app(&package.sources, &validated, &monotonic_shell()).unwrap();

    assert!(source.contains("use rtic_monotonics::systick::prelude::*;"));
    assert!(source.contains("systick_monotonic!(__FerroforgeMono, 1000u32);"));
    assert!(source.contains("crate::__FerroforgeMono::start(cx.core.SYST, 16_000_000);"));
    assert!(source.contains("crate::__FerroforgeMono::delay("));
    assert_eq!(source.matches("Mono::delay(").count(), 1);
    assert!(!source.contains("CONFIG.PERIOD_MS"));

    check_arm_source(&source);
}

#[test]
fn renderer_transplants_checked_native_init_into_the_real_rtic_app() {
    let init = discover_init_package(&init_manifest()).unwrap();
    let tasks = discover_task_package(&sw_manifest()).unwrap();
    let composition = StandaloneComposition {
        tasks: vec![
            TaskSelection {
                instance: "status".to_owned(),
                definition: id(&tasks.sources, &["layout"], "no_args"),
                priority: 1,
                local: Vec::new(),
                shared: Vec::new(),
                configuration: Vec::new(),
                spawn: Vec::new(),
            },
            TaskSelection {
                instance: "telemetry".to_owned(),
                definition: id(&tasks.sources, &["layout"], "consumer"),
                priority: 1,
                local: Vec::new(),
                shared: Vec::new(),
                configuration: Vec::new(),
                spawn: Vec::new(),
            },
        ],
        monotonic: Some(MonotonicProfile::initial_systick()),
    };
    let validated = validate_composition(&tasks.sources, &composition).unwrap();
    let source = render_rtic_app_with_init(&tasks.sources, &init, &validated, &target()).unwrap();

    assert_eq!(source.matches("struct Shared").count(), 1);
    assert_eq!(source.matches("struct Local").count(), 1);
    assert!(source.contains("cx.device.RCC.freeze"));
    assert!(source.contains("gpioa.pa5.into_push_pull_output()"));
    assert!(source.contains("crate::__FerroforgeMono::start(cx.core.SYST"));
    assert!(source.contains("status::spawn().unwrap()"));
    assert!(source.contains("telemetry::spawn(7).unwrap()"));
    assert!(source.contains("async fn status"));
    assert!(source.contains("async fn telemetry(_cx: telemetry::Context, value: u32)"));
    assert_eq!(source.matches("Mono::start").count(), 1);
    assert!(!source.contains("ferroforge::"));

    check_arm_source(&source);
}

#[test]
fn renderer_rewrites_mapped_native_logging_arguments() {
    let package = discover_task_package(&sw_manifest()).unwrap();
    let mut task = selection(
        "logger",
        id(&package.sources, &["layout"], "logged"),
        "state",
    );
    task.configuration.push(ConfigurationBinding {
        name: "step".to_owned(),
        rust_type: "u32".to_owned(),
        value: "4".to_owned(),
    });
    let composition = StandaloneComposition {
        tasks: vec![task],
        monotonic: None,
    };
    let validated = validate_composition(&package.sources, &composition).unwrap();
    let source =
        render_single_module_rtic_app(&package.sources, &validated, &logging_shell()).unwrap();

    assert!(source.contains("log_info!("));
    assert!(source.contains("rtt_log!("));
    assert!(source.contains("defmt::warn!("));
    assert_eq!(source.matches("CONFIG.STEP").count(), 1);
    assert_eq!(source.matches("cx.shared.sample").count(), 1);
    assert_eq!(
        source.matches("__ferroforge_config::logger::STEP").count(),
        2
    );
    assert!(source.contains("cx.shared.state.lock"));
    assert!(source.contains("literal CONFIG.STEP and cx.shared.sample; step={=u32}"));

    check_arm_source(&source);
}

#[test]
fn transplant_rejects_shadowed_context_and_escaping_relative_paths() {
    let package = discover_task_package(&sw_manifest()).unwrap();
    let composition = StandaloneComposition {
        tasks: vec![selection(
            "shadow",
            id(&package.sources, &["layout"], "shadows"),
            "state",
        )],
        monotonic: None,
    };
    let validated = validate_composition(&package.sources, &composition).unwrap();
    let error = render_single_module_rtic_app(&package.sources, &validated, &shell())
        .unwrap_err()
        .to_string();
    assert!(error.contains("shadows its context parameter"), "{error}");

    let composition = StandaloneComposition {
        tasks: vec![selection(
            "parent",
            id(&package.sources, &["layout"], "parent_ref"),
            "state",
        )],
        monotonic: None,
    };
    let validated = validate_composition(&package.sources, &composition).unwrap();
    let error = render_single_module_rtic_app(&package.sources, &validated, &shell())
        .unwrap_err()
        .to_string();
    assert!(error.contains("`super::` reference escapes"), "{error}");

    let composition = StandaloneComposition {
        tasks: vec![selection(
            "escape",
            id(&package.sources, &["layout"], "crate_escape"),
            "state",
        )],
        monotonic: None,
    };
    let validated = validate_composition(&package.sources, &composition).unwrap();
    let error = render_single_module_rtic_app(&package.sources, &validated, &shell())
        .unwrap_err()
        .to_string();
    assert!(error.contains("`crate::` reference escapes"), "{error}");

    for (definition, expected) in [
        ("config_bare", "uses bare `CONFIG`"),
        ("config_macro", "`CONFIG` inside a macro"),
    ] {
        let mut task = selection(
            "configured",
            id(&package.sources, &["layout"], definition),
            "state",
        );
        task.configuration.push(ConfigurationBinding {
            name: "step".to_owned(),
            rust_type: "u32".to_owned(),
            value: "2".to_owned(),
        });
        let composition = StandaloneComposition {
            tasks: vec![task],
            monotonic: None,
        };
        let validated = validate_composition(&package.sources, &composition).unwrap();
        let error = render_single_module_rtic_app(&package.sources, &validated, &shell())
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "{error}");
    }

    for (definition, expected) in [
        ("spawn_bare", "spawn handle outside a direct alias call"),
        ("spawn_macro", "spawn handle inside a macro"),
    ] {
        let composition = spawn_composition(&package.sources, definition);
        let validated = validate_composition(&package.sources, &composition).unwrap();
        let error = render_single_module_rtic_app(&package.sources, &validated, &spawn_shell())
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "{error}");
    }

    for (definition, expected) in [
        ("monotonic_bare", "monotonic outside a direct `delay` call"),
        ("monotonic_macro", "monotonic inside a macro"),
    ] {
        let composition = monotonic_composition(&package.sources, definition);
        let validated = validate_composition(&package.sources, &composition).unwrap();
        let error = render_single_module_rtic_app(&package.sources, &validated, &monotonic_shell())
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "{error}");
    }
}
