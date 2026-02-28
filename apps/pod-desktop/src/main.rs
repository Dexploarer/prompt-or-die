//! Prompt or Die — Engine Proof of Concept
//!
//! This demo creates a world with:
//! - 2 AI agents (scripted, not LLM yet) that wander and fight
//! - Several static obstacles
//! - All going through the same observe → decide → validate → execute pipeline
//!
//! No rendering yet — pure simulation, printed to stdout.

use glam::Vec2;
use pod_core::action::{Action, AgentConstraints};
use pod_core::agent::{Agent, AgentType};
use pod_core::component::*;
use pod_core::id::AgentId;
use pod_core::observation::Observation;
use pod_core::world::World;

// ============================================================
// WANDERER AGENT — simple AI that moves toward nearest entity
// ============================================================

struct WandererAgent {
    id: AgentId,
    name: String,
    constraints: AgentConstraints,
    last_observation: Option<Observation>,
    ticks_alive: u64,
}

impl WandererAgent {
    fn new(name: &str) -> Self {
        Self {
            id: AgentId::new(),
            name: name.to_string(),
            constraints: AgentConstraints::default(),
            last_observation: None,
            ticks_alive: 0,
        }
    }
}

impl Agent for WandererAgent {
    fn id(&self) -> AgentId {
        self.id
    }

    fn agent_type(&self) -> AgentType {
        AgentType::ScriptedNpc
    }

    fn observe(&mut self, observation: Observation) {
        self.last_observation = Some(observation);
    }

    fn decide(&mut self) -> Vec<Action> {
        self.ticks_alive += 1;

        let Some(obs) = &self.last_observation else {
            return vec![Action::Idle];
        };

        // Simple behavior: move toward the nearest hostile entity
        // If no hostile entities, wander in a circle
        let nearest_hostile = obs
            .visible_entities
            .iter()
            .find(|e| matches!(e.relationship, pod_core::observation::Relationship::Hostile));

        if let Some(hostile) = nearest_hostile {
            // Move toward hostile
            let direction = (hostile.position - obs.self_state.position).normalize_or_zero();
            let mut actions = vec![Action::Move { direction }];

            // Attack if close enough
            if hostile.distance < 40.0 {
                actions.push(Action::Attack);
            }

            // Taunt every 120 ticks (2 seconds)
            if self.ticks_alive % 120 == 0 {
                actions.push(Action::Speak {
                    message: format!("{}: I see you!", self.name),
                    volume: pod_core::action::SpeakVolume::Normal,
                });
            }

            actions
        } else {
            // Wander in a circle
            let angle = (self.ticks_alive as f32 * 0.02).sin();
            let direction = Vec2::new(angle.cos(), angle.sin());
            vec![Action::Move { direction }]
        }
    }

    fn constraints(&self) -> &AgentConstraints {
        &self.constraints
    }

    fn constraints_mut(&mut self) -> &mut AgentConstraints {
        &mut self.constraints
    }

    fn on_join(&mut self) {
        println!("🎮 {} joined the game ({})", self.name, self.id);
    }

    fn on_leave(&mut self) {
        println!("👋 {} left the game", self.name);
    }
}

// ============================================================
// MAIN — RUN THE SIMULATION
// ============================================================

fn main() {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    println!("╔══════════════════════════════════════╗");
    println!("║       PROMPT OR DIE — Engine v0.1    ║");
    println!("║       Agent-Native Game Engine        ║");
    println!("╚══════════════════════════════════════╝");
    println!();

    // Create world with deterministic seed
    let mut world = World::new(42);

    // Add two AI agents on different teams
    let agent_a = WandererAgent::new("Alpha");
    let agent_b = WandererAgent::new("Bravo");

    let (_, entity_a) = world.add_agent(Box::new(agent_a));
    let (_, entity_b) = world.add_agent(Box::new(agent_b));

    // Set teams (so they're hostile to each other)
    world
        .ecs
        .insert_one(
            entity_a,
            Label {
                name: "Alpha".into(),
                team: Team::Team(1),
            },
        )
        .unwrap();
    world
        .ecs
        .insert_one(
            entity_b,
            Label {
                name: "Bravo".into(),
                team: Team::Team(2),
            },
        )
        .unwrap();

    // Position them apart
    if let Ok(mut t) = world.ecs.get::<&mut Transform>(entity_a) {
        t.position = Vec2::new(-100.0, 0.0);
    }
    if let Ok(mut t) = world.ecs.get::<&mut Transform>(entity_b) {
        t.position = Vec2::new(100.0, 0.0);
    }

    // Spawn some obstacles
    for i in 0..5 {
        let x = (i as f32 - 2.0) * 80.0;
        world
            .spawn_at(x, 150.0)
            .with_collider(Collider::rect(30.0, 30.0))
            .with_color(30.0, 30.0, [0.5, 0.5, 0.5, 1.0])
            .with_label("Wall", Team::None)
            .build();
    }

    println!(
        "World initialized: {} entities, {} agents",
        world.entity_count(),
        world.agent_count()
    );
    println!("Running 300 ticks (5 seconds at 60 TPS)...");
    println!();

    // Run simulation
    for _ in 0..300 {
        let result = world.step();

        // Print state every 60 ticks (1 second)
        if result.tick % 60 == 0 {
            println!("── Tick {} ──", result.tick);

            // Print agent positions
            for slot in &world.agents {
                if let Some(entity) = slot.entity_id {
                    if let Ok(transform) = world.ecs.get::<&Transform>(entity) {
                        let health = world
                            .ecs
                            .get::<&Health>(entity)
                            .map(|h| format!("HP:{:.0}/{:.0}", h.current, h.max))
                            .unwrap_or_default();
                        let label = world
                            .ecs
                            .get::<&Label>(entity)
                            .map(|l| l.name.clone())
                            .unwrap_or_else(|_| "???".into());

                        println!(
                            "  {} @ ({:.1}, {:.1}) vel=({:.1}, {:.1}) {}",
                            label,
                            transform.position.x,
                            transform.position.y,
                            world
                                .ecs
                                .get::<&Velocity>(entity)
                                .map(|v| v.linear)
                                .unwrap_or(Vec2::ZERO)
                                .x,
                            world
                                .ecs
                                .get::<&Velocity>(entity)
                                .map(|v| v.linear)
                                .unwrap_or(Vec2::ZERO)
                                .y,
                            health,
                        );
                    }
                }
            }

            // Print events
            for event in &result.events {
                match &event.event {
                    pod_core::event::Event::AgentSpoke { message, .. } => {
                        println!("  💬 {}", message);
                    }
                    pod_core::event::Event::Sound { name, .. } => {
                        println!("  🔊 {}", name);
                    }
                    _ => {}
                }
            }

            println!(
                "  [{} actions processed, {} rejected, {} entities]",
                result.actions_processed, result.actions_rejected, result.entity_count
            );
            println!();
        }
    }

    // Final observation test — serialize what an agent sees
    println!("═══ FINAL OBSERVATION (what an agent sees) ═══");
    if let Some(slot) = world.agents.first() {
        if let Some(obs) = &slot.last_observation {
            println!("{}", obs.to_agent_prompt());
        }
    }

    println!();
    println!("✅ Prompt or Die engine core — operational.");
    println!("   Both agents ran through identical pipelines.");
    println!("   The engine never knew (or cared) if they were human or AI.");
}
