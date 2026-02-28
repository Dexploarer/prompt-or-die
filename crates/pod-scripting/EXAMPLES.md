# Script Examples

Complete examples of entity scripts for the Prompt or Die game engine.

## Example 1: Simple Health Monitor

```lua
-- Logs health status every tick
function on_spawn(entity)
    log.info("Entity spawned with health tracking")
end

function on_tick(entity, dt)
    local health = entity.get_health()
    local percent = (health.current / health.max) * 100

    if percent <= 0 then
        log.error("Entity is dead!")
    elseif percent <= 25 then
        log.warn("Critical health: " .. percent .. "%")
    elseif percent <= 50 then
        log.info("Low health: " .. percent .. "%")
    end
end

function on_damage(entity, amount, source)
    local health = entity.get_health()
    log.warn("Took " .. amount .. " damage! Health: " .. health.current)
end
```

## Example 2: Patrol Behavior

```lua
-- Patrols between waypoints using position API
local patrol_points = {
    {x = 0, y = 0},
    {x = 100, y = 0},
    {x = 100, y = 100},
    {x = 0, y = 100},
}

local current_waypoint = 1
local move_speed = 50.0  -- units per second

function on_spawn(entity)
    log.info("Patrol entity spawned, starting patrol")
end

function on_tick(entity, dt)
    local pos = entity.get_position()
    local target = patrol_points[current_waypoint]

    -- Calculate direction to waypoint
    local direction = math_api.direction(pos.x, pos.y, target.x, target.y)

    -- Move towards waypoint
    local new_x = pos.x + direction.x * move_speed * dt
    local new_y = pos.y + direction.y * move_speed * dt

    entity.set_position(new_x, new_y)

    -- Check if reached waypoint
    local dist = math_api.distance(pos.x, pos.y, target.x, target.y)
    if dist < 10.0 then
        current_waypoint = current_waypoint + 1
        if current_waypoint > #patrol_points then
            current_waypoint = 1
        end
        log.info("Reached waypoint " .. current_waypoint)
    end
end
```

## Example 3: Combat AI (Passive)

```lua
-- Aggressive entity that attacks nearby enemies
local detection_range = 200.0
local attack_cooldown = 0.5
local last_attack_time = 0.0

function on_spawn(entity)
    log.info("Combat entity spawned")
end

function on_tick(entity, dt)
    last_attack_time = last_attack_time - dt

    -- Find nearby enemies
    local pos = entity.get_position()
    local nearby = world.find_nearest(pos.x, pos.y, detection_range, "enemy")

    if #nearby > 0 and last_attack_time <= 0 then
        local target = nearby[1]
        log.info("Attacking enemy at " .. target.distance)

        -- Emit attack event
        events.emit("attack", {
            target_id = target.id,
            damage = 10.0
        })

        last_attack_time = attack_cooldown
    end
end
```

## Example 4: Stateful State Machine

```lua
-- Entity with states: Idle, Chase, Attack, Retreat
local state = "idle"
local state_timer = 0.0
local current_target = nil

function on_spawn(entity)
    state = "idle"
    state_timer = 0.0
    current_target = nil
    log.info("State machine entity spawned in idle state")
end

function on_tick(entity, dt)
    state_timer = state_timer - dt

    local pos = entity.get_position()
    local health = entity.get_health()
    local health_percent = health.current / health.max

    -- State transitions
    if state == "idle" then
        local nearby = world.find_nearest(pos.x, pos.y, 150, "enemy")
        if #nearby > 0 then
            current_target = nearby[1]
            state = "chase"
            log.info("Idle -> Chase")
        end

    elseif state == "chase" then
        if current_target then
            local dist = math_api.distance(pos.x, pos.y, current_target.x, current_target.y)

            if dist < 50 then
                state = "attack"
                state_timer = 0.3
                log.info("Chase -> Attack")
            else
                -- Move towards target
                local dir = math_api.direction(pos.x, pos.y, current_target.x, current_target.y)
                entity.set_position(pos.x + dir.x * 100 * dt, pos.y + dir.y * 100 * dt)
            end
        else
            state = "idle"
            log.info("Chase -> Idle (target lost)")
        end

    elseif state == "attack" then
        if state_timer <= 0 and current_target then
            events.emit("attack", {target_id = current_target.id, damage = 15})
            state_timer = 0.5
        end

        if health_percent < 0.3 then
            state = "retreat"
            state_timer = 3.0
            log.info("Attack -> Retreat (low health)")
        end

    elseif state == "retreat" then
        if state_timer <= 0 then
            state = "idle"
            log.info("Retreat -> Idle")
        else
            -- Move away from target
            if current_target then
                local dir = math_api.direction(current_target.x, current_target.y, pos.x, pos.y)
                entity.set_position(pos.x + dir.x * 150 * dt, pos.y + dir.y * 150 * dt)
            end
        end
    end
end
```

## Example 5: Collision Reaction

```lua
-- Entity that reacts to collisions
local collision_count = 0

function on_spawn(entity)
    collision_count = 0
    log.info("Collision-aware entity spawned")
end

function on_collision(entity, other)
    collision_count = collision_count + 1
    log.warn("Collision #" .. collision_count .. " with entity " .. other.id)

    if collision_count > 3 then
        log.error("Too many collisions, entity may be stuck!")
        events.emit("stuck_warning", {entity_id = entity.entity_id})
    end
end

function on_tick(entity, dt)
    -- Reset collision count every 5 seconds
    if time.tick() % 300 == 0 then
        collision_count = 0
        log.info("Collision counter reset")
    end
end
```

## Example 6: Resource Management

```lua
-- Entity that manages resources/inventory
local resources = {
    energy = 100,
    ammo = 50,
}

local max_resources = {
    energy = 100,
    ammo = 50,
}

function on_spawn(entity)
    log.info("Resource entity spawned with " .. resources.energy .. " energy")
end

function on_tick(entity, dt)
    -- Slow energy drain
    resources.energy = resources.energy - (10 * dt)

    -- Clamp resources
    if resources.energy < 0 then
        resources.energy = 0
        log.warn("Out of energy!")
    end

    -- Auto-regenerate ammo
    if resources.ammo < max_resources.ammo then
        resources.ammo = resources.ammo + (5 * dt)
        if resources.ammo > max_resources.ammo then
            resources.ammo = max_resources.ammo
        end
    end

    -- Log status every second (60 ticks)
    if time.tick() % 60 == 0 then
        log.info("Energy: " .. math.floor(resources.energy) ..
                 " Ammo: " .. math.floor(resources.ammo))
    end
end

function on_damage(entity, amount, source)
    -- Damage consumes energy
    local energy_cost = amount * 5
    resources.energy = resources.energy - energy_cost
    log.warn("Damage consumed " .. energy_cost .. " energy")
end
```

## Example 7: Spawner Behavior

```lua
-- Entity that spawns other entities
local spawn_interval = 2.0
local time_since_spawn = 0.0
local spawn_count = 0
local max_spawns = 10

function on_spawn(entity)
    spawn_count = 0
    time_since_spawn = 0.0
    log.info("Spawner created, will spawn up to " .. max_spawns .. " entities")
end

function on_tick(entity, dt)
    time_since_spawn = time_since_spawn + dt

    if time_since_spawn >= spawn_interval and spawn_count < max_spawns then
        local pos = entity.get_position()

        -- Spawn new entity with slight randomization
        local offset_x = math_api.random() * 20 - 10
        local offset_y = math_api.random() * 20 - 10

        local new_id = world.spawn("minion", pos.x + offset_x, pos.y + offset_y)
        spawn_count = spawn_count + 1

        log.info("Spawned entity #" .. spawn_count .. " with ID " .. new_id)

        time_since_spawn = 0
    end
end

function on_destroy(entity)
    log.warn("Spawner destroyed after creating " .. spawn_count .. " entities")
end
```

## Example 8: Environmental Hazard

```lua
-- Damage aura that hurts nearby entities
local damage_radius = 100.0
local damage_per_second = 5.0
local tick_damage = damage_per_second / 60.0  -- Convert to per-tick

function on_spawn(entity)
    log.info("Hazard spawned with " .. damage_radius .. " unit radius")
end

function on_tick(entity, dt)
    local pos = entity.get_position()

    -- Find all nearby entities (not just enemies)
    local nearby = world.find_nearest(pos.x, pos.y, damage_radius, "all")

    for i, target in ipairs(nearby) do
        if target.id ~= entity.entity_id then  -- Don't hurt self
            events.emit("damage", {
                target_id = target.id,
                amount = tick_damage,
                source_id = entity.entity_id,
                damage_type = "hazard"
            })
        end
    end
end
```

## Notes

These examples demonstrate:

1. **Health monitoring** — Tracking and logging entity stats
2. **Pathfinding** — Moving between waypoints using distance/direction
3. **AI behavior** — Detecting and attacking enemies
4. **State machines** — Complex behavior with multiple states
5. **Event handling** — Reacting to collisions
6. **Resource management** — Tracking internal state
7. **Spawning** — Creating new entities dynamically
8. **Environment hazards** — Damaging nearby entities

All examples use the safe, sandboxed API and will never cause the game engine to crash, even with incorrect logic.
