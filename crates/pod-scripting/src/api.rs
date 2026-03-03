//! Script API — functions exposed to Lua scripts

use glam::Vec2;
use mlua::{Lua, Table};
use serde_json::json;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex, OnceLock};

const SCRIPT_WORLD_BOUND: f32 = 10_000.0;
const MAX_SCRIPT_ENTITIES: usize = 10_000;
const MAX_SCRIPT_EVENTS_PER_TICK: usize = 64;

#[derive(Clone)]
struct ScriptEntity {
    entity_id: u64,
    type_name: String,
    position: Vec2,
}

struct RuntimeState {
    entities: HashMap<u64, ScriptEntity>,
    events: Vec<(u64, String, String)>,
    current_tick: u64,
    events_this_tick: usize,
}

impl RuntimeState {
    fn new() -> Self {
        Self {
            entities: HashMap::new(),
            events: Vec::new(),
            current_tick: 0,
            events_this_tick: 0,
        }
    }
}

static RUNTIME_STATE: OnceLock<Mutex<RuntimeState>> = OnceLock::new();
static NEXT_SCRIPT_ENTITY_ID: AtomicU64 = AtomicU64::new(1);

fn runtime_state() -> &'static Mutex<RuntimeState> {
    RUNTIME_STATE.get_or_init(|| Mutex::new(RuntimeState::new()))
}

/// Context passed to scripts for entity/world access
pub struct ScriptContext {
    /// Entity ID this script belongs to
    pub entity_id: u64,
    /// Entity position (read-only in most cases)
    pub position: Vec2,
    /// Entity rotation (radians)
    pub rotation: f32,
    /// Entity velocity
    pub velocity: Vec2,
    /// Entity health (current, max)
    pub health: (f32, f32),
    /// Current world tick
    pub tick: u64,
    /// Delta time since last frame
    pub dt: f32,
}

impl ScriptContext {
    pub fn new(entity_id: u64) -> Self {
        Self {
            entity_id,
            position: Vec2::ZERO,
            rotation: 0.0,
            velocity: Vec2::ZERO,
            health: (100.0, 100.0),
            tick: 0,
            dt: 1.0 / 60.0,
        }
    }
}

/// Build the Lua API tables
pub fn build_api(lua: &Lua, context: &ScriptContext) -> mlua::Result<Table> {
    let api_table = lua.create_table()?;

    // === ENTITY API ===
    let entity_table = lua.create_table()?;

    // entity.get_position() -> {x, y}
    let pos = context.position;
    let get_position = lua.create_function(move |lua, ()| {
        let t = lua.create_table()?;
        t.set("x", pos.x)?;
        t.set("y", pos.y)?;
        Ok(t)
    })?;
    entity_table.set("get_position", get_position)?;

    // entity.get_rotation() -> number
    let rot = context.rotation;
    let get_rotation = lua.create_function(move |_, ()| Ok(rot))?;
    entity_table.set("get_rotation", get_rotation)?;

    // entity.get_velocity() -> {x, y}
    let vel = context.velocity;
    let get_velocity = lua.create_function(move |lua, ()| {
        let t = lua.create_table()?;
        t.set("x", vel.x)?;
        t.set("y", vel.y)?;
        Ok(t)
    })?;
    entity_table.set("get_velocity", get_velocity)?;

    // entity.get_health() -> {current, max}
    let (current, max) = context.health;
    let get_health = lua.create_function(move |lua, ()| {
        let t = lua.create_table()?;
        t.set("current", current)?;
        t.set("max", max)?;
        Ok(t)
    })?;
    entity_table.set("get_health", get_health)?;

    // entity.set_position(x, y) — validates but returns ok (actual modification happens outside)
    let set_position = lua.create_function(|_, (x, y): (f32, f32)| {
        // Validation only — caller handles actual mutation
        if x.is_finite() && y.is_finite() {
            Ok(())
        } else {
            Err(mlua::Error::external("Position values must be finite"))
        }
    })?;
    entity_table.set("set_position", set_position)?;

    api_table.set("entity", entity_table)?;

    // === WORLD API ===
    let world_table = lua.create_table()?;

    // world.spawn(type, x, y) -> entity_id
    let spawn = lua.create_function(|_, (type_name, x, y): (String, f32, f32)| {
        if type_name.trim().is_empty() {
            return Err(mlua::Error::external("world.spawn type name cannot be empty"));
        }
        if !x.is_finite() || !y.is_finite() {
            return Err(mlua::Error::external(
                "world.spawn coordinates must be finite values",
            ));
        }
        if x.abs() > SCRIPT_WORLD_BOUND || y.abs() > SCRIPT_WORLD_BOUND {
            return Err(mlua::Error::external(
                "world.spawn coordinates exceed script world bounds",
            ));
        }

        let entity_id = NEXT_SCRIPT_ENTITY_ID.fetch_add(1, Ordering::SeqCst);
        let mut state = runtime_state()
            .lock()
            .map_err(|_| mlua::Error::external("world.spawn failed to lock runtime state"))?;
        if state.entities.len() >= MAX_SCRIPT_ENTITIES {
            return Err(mlua::Error::external(
                "world.spawn entity limit reached for script runtime",
            ));
        }

        state.entities.insert(
            entity_id,
            ScriptEntity {
                entity_id,
                type_name,
                position: Vec2::new(x, y),
            },
        );

        Ok(entity_id)
    })?;
    world_table.set("spawn", spawn)?;

    // world.destroy(entity_id)
    let destroy = lua.create_function(|_, id: u64| {
        if id == 0 {
            return Err(mlua::Error::external("world.destroy entity id must be non-zero"));
        }
        let mut state = runtime_state()
            .lock()
            .map_err(|_| mlua::Error::external("world.destroy failed to lock runtime state"))?;
        if state.entities.remove(&id).is_none() {
            return Err(mlua::Error::external(format!(
                "world.destroy unknown entity id {id}"
            )));
        }
        Ok(())
    })?;
    world_table.set("destroy", destroy)?;

    // world.find_nearest(x, y, radius, tag) -> [{id, x, y, distance}]
    let find_nearest = lua.create_function(|lua, (x, y, radius, tag): (f32, f32, f32, String)| {
        if !x.is_finite() || !y.is_finite() || !radius.is_finite() {
            return Err(mlua::Error::external(
                "world.find_nearest inputs must be finite values",
            ));
        }
        if radius < 0.0 {
            return Err(mlua::Error::external(
                "world.find_nearest radius must be non-negative",
            ));
        }

        let tag = tag.trim().to_string();
        let state = runtime_state()
            .lock()
            .map_err(|_| mlua::Error::external("world.find_nearest failed to lock runtime state"))?;
        let mut matches: Vec<(u64, String, f32, f32, f32)> = state
            .entities
            .values()
            .filter_map(|entity| {
                if !tag.is_empty() && entity.type_name != tag {
                    return None;
                }
                let dx = entity.position.x - x;
                let dy = entity.position.y - y;
                let dist = (dx * dx + dy * dy).sqrt();
                (dist <= radius).then_some((
                    entity.entity_id,
                    entity.type_name.clone(),
                    entity.position.x,
                    entity.position.y,
                    dist,
                ))
            })
            .collect();
        matches.sort_by(|a, b| {
            a.4.partial_cmp(&b.4)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });

        let result = lua.create_table()?;
        for (index, (entity_id, type_name, ex, ey, distance)) in matches.into_iter().take(32).enumerate() {
            let row = lua.create_table()?;
            row.set("id", entity_id)?;
            row.set("type", type_name)?;
            row.set("x", ex)?;
            row.set("y", ey)?;
            row.set("distance", distance)?;
            result.set(index + 1, row)?;
        }
        Ok(result)
    })?;
    world_table.set("find_nearest", find_nearest)?;

    api_table.set("world", world_table)?;

    // === EVENTS API ===
    let events_table = lua.create_table()?;

    // events.emit(name, data)
    let tick = context.tick;
    let emit = lua.create_function(move |_, (name, data): (String, mlua::Value)| {
        if name.is_empty() {
            return Err(mlua::Error::external("Event name cannot be empty"));
        }
        if name.len() > 64 {
            return Err(mlua::Error::external("Event name cannot exceed 64 characters"));
        }

        let payload = serde_json::to_string(&data).unwrap_or_else(|_| {
            json!({ "error": "unserializable_event_payload" }).to_string()
        });

        let mut state = runtime_state()
            .lock()
            .map_err(|_| mlua::Error::external("events.emit failed to lock runtime state"))?;
        if state.current_tick != tick {
            state.current_tick = tick;
            state.events_this_tick = 0;
        }
        if state.events_this_tick >= MAX_SCRIPT_EVENTS_PER_TICK {
            return Err(mlua::Error::external(
                "events.emit rate limit exceeded for this tick",
            ));
        }
        state.events_this_tick += 1;
        state.events.push((tick, name, payload));
        Ok(())
    })?;
    events_table.set("emit", emit)?;

    api_table.set("events", events_table)?;

    // === TIME API ===
    let time_table = lua.create_table()?;

    // time.tick() -> u64
    let tick = context.tick;
    let get_tick = lua.create_function(move |_, ()| Ok(tick))?;
    time_table.set("tick", get_tick)?;

    // time.dt() -> f32
    let dt = context.dt;
    let get_dt = lua.create_function(move |_, ()| Ok(dt))?;
    time_table.set("dt", get_dt)?;

    api_table.set("time", time_table)?;

    // === MATH API (custom, not stdlib) ===
    let math_api_table = lua.create_table()?;

    // math.distance(x1, y1, x2, y2) -> number
    let distance = lua.create_function(|_, (x1, y1, x2, y2): (f32, f32, f32, f32)| {
        let dx = x2 - x1;
        let dy = y2 - y1;
        Ok((dx * dx + dy * dy).sqrt())
    })?;
    math_api_table.set("distance", distance)?;

    // math.direction(x1, y1, x2, y2) -> {x, y} normalized
    let direction = lua.create_function(|lua, (x1, y1, x2, y2): (f32, f32, f32, f32)| {
        let dx = x2 - x1;
        let dy = y2 - y1;
        let len = (dx * dx + dy * dy).sqrt();

        let (nx, ny) = if len > 0.0 {
            (dx / len, dy / len)
        } else {
            (0.0, 0.0)
        };

        let t = lua.create_table()?;
        t.set("x", nx)?;
        t.set("y", ny)?;
        Ok(t)
    })?;
    math_api_table.set("direction", direction)?;

    // math.random() -> 0..1 (deterministic RNG from engine)
    let rng = Arc::new(AtomicU32::new(0x12345678));
    let rng_clone = rng.clone();
    let random = lua.create_function(move |_, ()| {
        // Linear congruential generator (deterministic, not cryptographic)
        let prev = rng_clone.load(Ordering::SeqCst);
        let next = prev.wrapping_mul(1103515245).wrapping_add(12345);
        rng_clone.store(next, Ordering::SeqCst);
        Ok((next as f32 / u32::MAX as f32).abs())
    })?;
    math_api_table.set("random", random)?;

    api_table.set("math_api", math_api_table)?;

    // === LOGGING API ===
    let log_table = lua.create_table()?;

    let log_info = lua.create_function(|_, msg: String| {
        log::info!("[Script] {}", msg);
        Ok(())
    })?;
    log_table.set("info", log_info)?;

    let log_warn = lua.create_function(|_, msg: String| {
        log::warn!("[Script] {}", msg);
        Ok(())
    })?;
    log_table.set("warn", log_warn)?;

    let log_error = lua.create_function(|_, msg: String| {
        log::error!("[Script] {}", msg);
        Ok(())
    })?;
    log_table.set("error", log_error)?;

    api_table.set("log", log_table)?;

    Ok(api_table)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_creation() {
        let ctx = ScriptContext::new(42);
        assert_eq!(ctx.entity_id, 42);
        assert_eq!(ctx.position, Vec2::ZERO);
    }

    #[test]
    fn test_api_building() {
        let lua = Lua::new();
        let ctx = ScriptContext::new(1);
        let api = build_api(&lua, &ctx).expect("API build failed");
        assert!(!api.is_empty());
    }
}
