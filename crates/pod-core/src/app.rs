use std::any::{Any, TypeId};
use std::collections::HashMap;

use crate::action::AgentAction;
use crate::component::{
    AgentControlled, ColorRect, CombatLoadout, CompanionRoster, CreatureIdentity, EncounterState,
    Health, Inventory, Label, LootContainer, Movement, Perception, ResourceNode, Script, SkillBook,
    Sprite, Transform, Transform3D, Velocity,
};
use crate::contract::{
    ToolBudget, ToolCatalog, ToolDefinition, ToolInvocationRequest, ToolInvocationResult,
    ToolPolicy, VersionedAgentAction, VersionedObservation, VersionedTickTelemetry,
};
use crate::observation::Observation;
use crate::telemetry::{TelemetryArchive, TelemetryConfig, TickTelemetryFrame};
use crate::tick::TickResult;
use crate::World;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SchedulePhase {
    Startup,
    PreTick,
    PostTick,
    RenderPrep,
    Broadcast,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RegisteredTypeCategory {
    Component,
    Resource,
    Contract,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeMetadata {
    pub key: &'static str,
    pub rust_type_name: &'static str,
    pub category: RegisteredTypeCategory,
}

#[derive(Default)]
pub struct TypeRegistry {
    entries: HashMap<TypeId, TypeMetadata>,
}

impl TypeRegistry {
    pub fn register_component<T: 'static>(&mut self, key: &'static str) {
        self.entries.insert(
            TypeId::of::<T>(),
            TypeMetadata {
                key,
                rust_type_name: std::any::type_name::<T>(),
                category: RegisteredTypeCategory::Component,
            },
        );
    }

    pub fn register_resource<T: 'static>(&mut self, key: &'static str) {
        self.entries.insert(
            TypeId::of::<T>(),
            TypeMetadata {
                key,
                rust_type_name: std::any::type_name::<T>(),
                category: RegisteredTypeCategory::Resource,
            },
        );
    }

    pub fn register_contract<T: 'static>(&mut self, key: &'static str) {
        self.entries.insert(
            TypeId::of::<T>(),
            TypeMetadata {
                key,
                rust_type_name: std::any::type_name::<T>(),
                category: RegisteredTypeCategory::Contract,
            },
        );
    }

    pub fn metadata<T: 'static>(&self) -> Option<&TypeMetadata> {
        self.entries.get(&TypeId::of::<T>())
    }
}

#[derive(Default)]
pub struct ResourceStore {
    entries: HashMap<TypeId, Box<dyn Any + Send>>,
}

impl ResourceStore {
    pub fn insert<T: Send + 'static>(&mut self, value: T) -> Option<T> {
        self.entries
            .insert(TypeId::of::<T>(), Box::new(value))
            .and_then(|old| old.downcast::<T>().ok())
            .map(|boxed| *boxed)
    }

    pub fn get<T: Send + 'static>(&self) -> Option<&T> {
        self.entries
            .get(&TypeId::of::<T>())
            .and_then(|value| value.downcast_ref::<T>())
    }

    pub fn get_mut<T: Send + 'static>(&mut self) -> Option<&mut T> {
        self.entries
            .get_mut(&TypeId::of::<T>())
            .and_then(|value| value.downcast_mut::<T>())
    }
}

pub struct AppContext<'a> {
    pub world: &'a mut World,
    pub resources: &'a mut ResourceStore,
    pub types: &'a mut TypeRegistry,
}

impl<'a> AppContext<'a> {
    pub fn insert_resource<T: Send + 'static>(&mut self, value: T) {
        self.types
            .register_resource::<T>(std::any::type_name::<T>());
        let _ = self.resources.insert(value);
    }

    pub fn resource<T: Send + 'static>(&self) -> Option<&T> {
        self.resources.get::<T>()
    }

    pub fn resource_mut<T: Send + 'static>(&mut self) -> Option<&mut T> {
        self.resources.get_mut::<T>()
    }
}

type SystemFn = Box<dyn FnMut(&mut AppContext<'_>) + Send>;

struct SystemEntry {
    name: String,
    system: SystemFn,
}

pub trait Plugin: Send + Sync {
    fn build(&self, app: &mut App);
}

#[derive(Debug, Clone)]
pub struct LastTickResult(pub TickResult);

pub struct App {
    world: World,
    resources: ResourceStore,
    types: TypeRegistry,
    startup_ran: bool,
    startup: Vec<SystemEntry>,
    pre_tick: Vec<SystemEntry>,
    post_tick: Vec<SystemEntry>,
    render_prep: Vec<SystemEntry>,
    broadcast: Vec<SystemEntry>,
}

impl App {
    pub fn new(seed: u64) -> Self {
        let mut app = Self {
            world: World::new(seed),
            resources: ResourceStore::default(),
            types: TypeRegistry::default(),
            startup_ran: false,
            startup: Vec::new(),
            pre_tick: Vec::new(),
            post_tick: Vec::new(),
            render_prep: Vec::new(),
            broadcast: Vec::new(),
        };
        app.register_core_types();
        let telemetry_config = TelemetryConfig::default();
        let _ = app.resources.insert(telemetry_config);
        let _ = app.resources.insert(TelemetryArchive::with_capacity(
            telemetry_config.core_archive_ticks,
        ));
        app
    }

    pub fn world(&self) -> &World {
        &self.world
    }

    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }

    pub fn type_registry(&self) -> &TypeRegistry {
        &self.types
    }

    pub fn resources(&self) -> &ResourceStore {
        &self.resources
    }

    pub fn resources_mut(&mut self) -> &mut ResourceStore {
        &mut self.resources
    }

    pub fn insert_resource<T: Send + 'static>(&mut self, value: T) -> &mut Self {
        self.types
            .register_resource::<T>(std::any::type_name::<T>());
        let _ = self.resources.insert(value);
        self
    }

    pub fn add_plugin<P: Plugin + 'static>(&mut self, plugin: P) -> &mut Self {
        plugin.build(self);
        self
    }

    pub fn add_system<F>(
        &mut self,
        phase: SchedulePhase,
        name: impl Into<String>,
        system: F,
    ) -> &mut Self
    where
        F: FnMut(&mut AppContext<'_>) + Send + 'static,
    {
        self.systems_for_phase_mut(phase).push(SystemEntry {
            name: name.into(),
            system: Box::new(system),
        });
        self
    }

    pub fn update(&mut self) -> TickResult {
        self.ensure_started();
        self.run_phase(SchedulePhase::PreTick);

        let result = self.world.step();
        let _ = self.resources.insert(LastTickResult(result.clone()));
        if let Some(archive) = self.resources.get_mut::<TelemetryArchive>() {
            archive.record_tick(result.telemetry.clone());
        }

        self.run_phase(SchedulePhase::PostTick);
        self.run_phase(SchedulePhase::Broadcast);
        result
    }

    pub fn prepare_render(&mut self) {
        self.ensure_started();
        self.run_phase(SchedulePhase::RenderPrep);
    }

    fn ensure_started(&mut self) {
        if self.startup_ran {
            return;
        }

        self.run_phase(SchedulePhase::Startup);
        self.startup_ran = true;
    }

    fn run_phase(&mut self, phase: SchedulePhase) {
        let mut systems = std::mem::take(self.systems_for_phase_mut(phase));
        {
            let mut ctx = AppContext {
                world: &mut self.world,
                resources: &mut self.resources,
                types: &mut self.types,
            };

            for entry in systems.iter_mut() {
                log::trace!("running {:?} system {}", phase, entry.name);
                (entry.system)(&mut ctx);
            }
        }
        *self.systems_for_phase_mut(phase) = systems;
    }

    fn systems_for_phase_mut(&mut self, phase: SchedulePhase) -> &mut Vec<SystemEntry> {
        match phase {
            SchedulePhase::Startup => &mut self.startup,
            SchedulePhase::PreTick => &mut self.pre_tick,
            SchedulePhase::PostTick => &mut self.post_tick,
            SchedulePhase::RenderPrep => &mut self.render_prep,
            SchedulePhase::Broadcast => &mut self.broadcast,
        }
    }

    fn register_core_types(&mut self) {
        self.types.register_component::<Transform>("Transform");
        self.types.register_component::<Transform3D>("Transform3D");
        self.types.register_component::<Velocity>("Velocity");
        self.types.register_component::<Sprite>("Sprite");
        self.types.register_component::<ColorRect>("ColorRect");
        self.types
            .register_component::<AgentControlled>("AgentControlled");
        self.types.register_component::<Health>("Health");
        self.types.register_component::<Label>("Label");
        self.types.register_component::<Perception>("Perception");
        self.types.register_component::<Script>("Script");
        self.types.register_component::<Movement>("Movement");
        self.types
            .register_component::<CombatLoadout>("CombatLoadout");
        self.types.register_component::<SkillBook>("SkillBook");
        self.types.register_component::<Inventory>("Inventory");
        self.types
            .register_component::<CompanionRoster>("CompanionRoster");
        self.types
            .register_component::<CreatureIdentity>("CreatureIdentity");
        self.types
            .register_component::<EncounterState>("EncounterState");
        self.types
            .register_component::<ResourceNode>("ResourceNode");
        self.types
            .register_component::<LootContainer>("LootContainer");
        self.types.register_contract::<Observation>("Observation");
        self.types.register_contract::<AgentAction>("AgentAction");
        self.types
            .register_contract::<ToolDefinition>("ToolDefinition");
        self.types.register_contract::<ToolCatalog>("ToolCatalog");
        self.types.register_contract::<ToolPolicy>("ToolPolicy");
        self.types.register_contract::<ToolBudget>("ToolBudget");
        self.types
            .register_contract::<ToolInvocationRequest>("ToolInvocationRequest");
        self.types
            .register_contract::<ToolInvocationResult>("ToolInvocationResult");
        self.types
            .register_contract::<TickTelemetryFrame>("TickTelemetryFrame");
        self.types
            .register_contract::<VersionedObservation>("VersionedObservation");
        self.types
            .register_contract::<VersionedAgentAction>("VersionedAgentAction");
        self.types
            .register_contract::<VersionedTickTelemetry>("VersionedTickTelemetry");
        self.types
            .register_resource::<TelemetryArchive>("TelemetryArchive");
        self.types
            .register_resource::<TelemetryConfig>("TelemetryConfig");
    }
}

#[cfg(test)]
mod tests {
    use super::{App, LastTickResult, Plugin, RegisteredTypeCategory, SchedulePhase};
    use crate::telemetry::{TelemetryArchive, TelemetryConfig};

    struct TracePlugin;

    impl Plugin for TracePlugin {
        fn build(&self, app: &mut App) {
            app.add_system(SchedulePhase::Startup, "startup", |ctx| {
                ctx.insert_resource(Vec::<String>::new());
                ctx.resource_mut::<Vec<String>>()
                    .expect("trace resource")
                    .push("startup".into());
            });
            app.add_system(SchedulePhase::PreTick, "pre", |ctx| {
                ctx.resource_mut::<Vec<String>>()
                    .expect("trace resource")
                    .push("pre".into());
            });
            app.add_system(SchedulePhase::PostTick, "post", |ctx| {
                ctx.resource_mut::<Vec<String>>()
                    .expect("trace resource")
                    .push("post".into());
            });
            app.add_system(SchedulePhase::Broadcast, "broadcast", |ctx| {
                let tick = ctx
                    .resource::<LastTickResult>()
                    .expect("last tick result")
                    .0
                    .tick;
                ctx.resource_mut::<Vec<String>>()
                    .expect("trace resource")
                    .push(format!("broadcast:{tick}"));
            });
            app.add_system(SchedulePhase::RenderPrep, "render", |ctx| {
                ctx.resource_mut::<Vec<String>>()
                    .expect("trace resource")
                    .push("render".into());
            });
        }
    }

    #[test]
    fn app_runs_startup_once_and_keeps_schedule_order_deterministic() {
        let mut app = App::new(42);
        app.add_plugin(TracePlugin);

        let first = app.update();
        assert_eq!(first.tick, 0);

        app.prepare_render();

        let second = app.update();
        assert_eq!(second.tick, 1);

        let trace = app
            .resources()
            .get::<Vec<String>>()
            .expect("trace resource should exist");
        assert_eq!(
            trace,
            &vec![
                "startup".to_string(),
                "pre".to_string(),
                "post".to_string(),
                "broadcast:0".to_string(),
                "render".to_string(),
                "pre".to_string(),
                "post".to_string(),
                "broadcast:1".to_string(),
            ]
        );
    }

    #[test]
    fn app_registers_core_contracts_and_components() {
        let app = App::new(7);
        let transform = app
            .type_registry()
            .metadata::<crate::component::Transform>()
            .expect("transform metadata");
        assert_eq!(transform.category, RegisteredTypeCategory::Component);

        let observation = app
            .type_registry()
            .metadata::<crate::observation::Observation>()
            .expect("observation metadata");
        assert_eq!(observation.category, RegisteredTypeCategory::Contract);

        let archive = app
            .resources()
            .get::<TelemetryArchive>()
            .expect("telemetry archive resource");
        assert!(archive.latest().is_none());
        let telemetry_config = app
            .resources()
            .get::<TelemetryConfig>()
            .expect("telemetry config resource");
        assert_eq!(telemetry_config.core_archive_ticks, 600);
    }

    #[test]
    fn app_records_tick_telemetry_into_archive() {
        let mut app = App::new(11);

        let result = app.update();

        let archive = app
            .resources()
            .get::<TelemetryArchive>()
            .expect("telemetry archive resource");
        let latest = archive.latest().expect("latest tick telemetry");
        assert_eq!(latest.tick, result.tick);
        assert_eq!(latest.tick, 0);
    }
}
