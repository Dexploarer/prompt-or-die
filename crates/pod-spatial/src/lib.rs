//! # pod-spatial — Spatial Indexing & Pathfinding Layer
//!
//! Provides high-performance spatial queries and AI pathfinding for the Prompt or Die engine.
//!
//! ## Features
//! - **SpatialIndex**: R-tree backed spatial index for O(log n) radius and rectangular queries
//! - **UniformGrid**: Broad-phase grid hashing for fast neighbor queries
//! - **NavMesh**: Precomputed 2D navigation mesh with walkability checks
//! - **GridPathfinder**: Simple grid-based A* pathfinding
//! - **RayCast**: 2D raycasting against spatial index

use glam::Vec2;
use log::{debug, warn};
use std::collections::{HashMap, HashSet};
use std::cmp::Ordering;

pub use pod_core::{Collider, ColliderShape, Transform};

// ============================================================
// SPATIAL INDEX (R-TREE BACKED)
// ============================================================

/// A 2D point in the R-tree spatial index
#[derive(Clone, Copy, Debug)]
pub struct RTreePoint {
    pub entity_id: u64,
    pub x: f32,
    pub y: f32,
    pub radius: f32,
}

impl rstar::Point for RTreePoint {
    type Scalar = f32;
    const DIMENSIONS: usize = 2;

    fn generate(dimension: usize, generator: impl Fn(usize) -> Self::Scalar) -> Self {
        let x = generator(0);
        let y = generator(1);
        Self {
            entity_id: 0,
            x,
            y,
            radius: 0.0,
        }
    }

    fn nth(&self, index: usize) -> Self::Scalar {
        match index {
            0 => self.x,
            1 => self.y,
            _ => 0.0,
        }
    }

    fn nth_mut(&mut self, index: usize) -> &mut Self::Scalar {
        match index {
            0 => &mut self.x,
            1 => &mut self.y,
            _ => &mut self.radius, // fallback, shouldn't happen
        }
    }
}

/// Result of a spatial query
#[derive(Debug, Clone)]
pub struct SpatialQueryResult {
    pub entity_id: u64,
    pub position: Vec2,
    pub distance: f32,
}

/// Fast R-tree based spatial index for queries
#[derive(Clone, Debug)]
pub struct SpatialIndex {
    tree: rstar::RTree<RTreePoint>,
    entity_positions: HashMap<u64, Vec2>,
}

impl Default for SpatialIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl SpatialIndex {
    /// Create a new empty spatial index
    pub fn new() -> Self {
        Self {
            tree: rstar::RTree::new(),
            entity_positions: HashMap::new(),
        }
    }

    /// Insert or update an entity in the index
    pub fn insert(&mut self, entity_id: u64, position: Vec2, radius: f32) {
        self.entity_positions.insert(entity_id, position);

        let point = RTreePoint {
            entity_id,
            x: position.x,
            y: position.y,
            radius,
        };

        // Use rstar's insert method for O(log n) insertion
        self.tree.insert(point);
    }

    /// Remove an entity from the index
    pub fn remove(&mut self, entity_id: u64) -> bool {
        if let Some(pos) = self.entity_positions.remove(&entity_id) {
            // Remove from R-tree as well
            let point = RTreePoint {
                entity_id,
                x: pos.x,
                y: pos.y,
                radius: 0.0,
            };
            self.tree.remove(&point);
            true
        } else {
            false
        }
    }

    /// Query all entities within a radius of a center point
    pub fn query_radius(&self, center: Vec2, radius: f32) -> Vec<SpatialQueryResult> {
        let min_x = center.x - radius;
        let min_y = center.y - radius;
        let max_x = center.x + radius;
        let max_y = center.y + radius;

        let mut results = Vec::new();

        for point in self.tree.locate_in_envelope_intersecting(&[
            (min_x, max_x),
            (min_y, max_y),
        ]) {
            let pos = self.entity_positions.get(&point.entity_id).copied();
            if let Some(pos) = pos {
                let dist = center.distance(pos);
                if dist <= radius {
                    results.push(SpatialQueryResult {
                        entity_id: point.entity_id,
                        position: pos,
                        distance: dist,
                    });
                }
            }
        }

        results.sort_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap_or(Ordering::Equal));
        results
    }

    /// Query all entities in a rectangular region
    pub fn query_rect(&self, min: Vec2, max: Vec2) -> Vec<u64> {
        self.tree
            .locate_in_envelope_intersecting(&[
                (min.x, max.x),
                (min.y, max.y),
            ])
            .map(|p| p.entity_id)
            .collect()
    }

    /// Find the nearest entities to a position
    pub fn query_nearest(&self, position: Vec2, count: usize) -> Vec<SpatialQueryResult> {
        let point = RTreePoint {
            entity_id: 0,
            x: position.x,
            y: position.y,
            radius: 0.0,
        };

        let mut results: Vec<_> = self
            .tree
            .nearest_neighbor_iter(&point)
            .take(count)
            .filter_map(|p| {
                self.entity_positions.get(&p.entity_id).map(|&pos| {
                    SpatialQueryResult {
                        entity_id: p.entity_id,
                        position: pos,
                        distance: position.distance(pos),
                    }
                })
            })
            .collect();

        results.sort_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap_or(Ordering::Equal));
        results
    }

    /// Rebuild the entire R-tree from internal positions
    pub fn rebuild(&mut self) {
        let points: Vec<RTreePoint> = self
            .entity_positions
            .iter()
            .map(|(&entity_id, &pos)| RTreePoint {
                entity_id,
                x: pos.x,
                y: pos.y,
                radius: 0.0,
            })
            .collect();

        self.tree = rstar::RTree::bulk_load(points);
    }

    /// Rebuild index from hecs ECS world (assumes Transform + Collider components)
    pub fn rebuild_from_ecs(&mut self, world: &hecs::World) {
        self.entity_positions.clear();

        for (id, (transform, _collider)) in world.query::<(&Transform, &Collider)>().iter() {
            let entity_id = id.id() as u64;
            self.entity_positions.insert(entity_id, transform.position);
        }

        self.rebuild();
    }

    /// Get the current position of an entity if it exists
    pub fn get_position(&self, entity_id: u64) -> Option<Vec2> {
        self.entity_positions.get(&entity_id).copied()
    }

    /// Clear all entities from the index
    pub fn clear(&mut self) {
        self.entity_positions.clear();
        self.tree = rstar::RTree::new();
    }

    /// Get count of indexed entities
    pub fn len(&self) -> usize {
        self.entity_positions.len()
    }

    /// Check if index is empty
    pub fn is_empty(&self) -> bool {
        self.entity_positions.is_empty()
    }
}

// ============================================================
// UNIFORM GRID (BROAD-PHASE SPATIAL HASHING)
// ============================================================

/// Cell-based spatial hash grid for broad-phase collision detection
#[derive(Clone, Debug)]
pub struct UniformGrid {
    cell_size: f32,
    cells: HashMap<(i32, i32), HashSet<u64>>,
}

impl UniformGrid {
    /// Create a new uniform grid with specified cell size
    pub fn new(cell_size: f32) -> Self {
        if cell_size <= 0.0 {
            warn!("UniformGrid cell_size must be positive, using default 10.0");
        }
        Self {
            cell_size: cell_size.max(0.1),
            cells: HashMap::new(),
        }
    }

    /// Get the grid cell coordinates for a world position
    fn get_cell(&self, position: Vec2) -> (i32, i32) {
        (
            (position.x / self.cell_size).floor() as i32,
            (position.y / self.cell_size).floor() as i32,
        )
    }

    /// Insert an entity at a position
    pub fn insert(&mut self, entity_id: u64, position: Vec2) {
        let cell = self.get_cell(position);
        self.cells.entry(cell).or_insert_with(HashSet::new).insert(entity_id);
    }

    /// Get all entities in a specific cell
    pub fn query_cell(&self, cell_x: i32, cell_y: i32) -> Vec<u64> {
        self.cells
            .get(&(cell_x, cell_y))
            .map(|set| set.iter().copied().collect())
            .unwrap_or_default()
    }

    /// Get all entities in the cell and 8 neighboring cells
    pub fn query_neighbors(&self, position: Vec2) -> Vec<u64> {
        let (cx, cy) = self.get_cell(position);
        let mut result = HashSet::new();

        // 3x3 neighborhood
        for dx in -1..=1 {
            for dy in -1..=1 {
                let cell = (cx + dx, cy + dy);
                if let Some(entities) = self.cells.get(&cell) {
                    result.extend(entities.iter().copied());
                }
            }
        }

        result.into_iter().collect()
    }

    /// Clear all entities from the grid
    pub fn clear(&mut self) {
        self.cells.clear();
    }

    /// Rebuild the entire grid (call after bulk position updates)
    pub fn rebuild(&mut self, positions: &[(u64, Vec2)]) {
        self.clear();
        for &(entity_id, position) in positions {
            self.insert(entity_id, position);
        }
    }

    /// Get count of indexed entities
    pub fn len(&self) -> usize {
        self.cells.values().map(|s| s.len()).sum()
    }

    /// Check if grid is empty
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// Get the cell size used by this grid
    pub fn cell_size(&self) -> f32 {
        self.cell_size
    }
}

// ============================================================
// RAY CASTING
// ============================================================

/// Result of a raycast operation
#[derive(Debug, Clone)]
pub struct RayHit {
    pub entity_id: u64,
    pub point: Vec2,
    pub normal: Vec2,
    pub distance: f32,
}

/// Perform a 2D raycast against the spatial index
pub fn ray_cast(
    origin: Vec2,
    direction: Vec2,
    max_distance: f32,
    spatial_index: &SpatialIndex,
) -> Option<RayHit> {
    // Handle zero-length direction
    if direction.length_squared() < 0.0001 {
        return None;
    }

    let dir_norm = direction.normalize();

    // Query a bounding box along the ray direction
    let query_radius = max_distance + 10.0; // Add buffer for collider radius
    let candidates = spatial_index.query_radius(origin, query_radius);

    let mut closest_hit: Option<RayHit> = None;
    let mut closest_dist = f32::INFINITY;

    for candidate_result in candidates {
        let pos = candidate_result.position;

        // Simple sphere-ray intersection test
        let to_point = pos - origin;
        let projection = to_point.dot(dir_norm);

        // Check if projection is within ray bounds
        if projection < 0.0 || projection > max_distance {
            continue;
        }

        let closest_point = origin + dir_norm * projection;
        let distance_to_ray = (pos - closest_point).length();

        // Assume small collision radius (1.0 units)
        if distance_to_ray < 1.0 && projection < closest_dist {
            closest_dist = projection;
            let normal = (closest_point - pos).normalize_or_zero();
            closest_hit = Some(RayHit {
                entity_id: candidate_result.entity_id,
                point: closest_point,
                normal,
                distance: projection,
            });
        }
    }

    closest_hit
}

// ============================================================
// NAVMESH (OBSTACLE-BASED GENERATION)
// ============================================================

/// 2D Navigation mesh for pathfinding with obstacle awareness
#[derive(Clone, Debug)]
pub struct NavMesh {
    /// Axis-aligned bounding boxes representing obstacles
    obstacles: Vec<(Vec2, Vec2)>, // (min, max) for each obstacle
    /// Bounds of the walkable area
    bounds: (Vec2, Vec2),
    /// Grid resolution for walkability queries
    grid_resolution: f32,
}

impl NavMesh {
    /// Create a navigation mesh from a list of axis-aligned obstacles
    pub fn new(obstacles: Vec<(Vec2, Vec2)>, bounds: (Vec2, Vec2)) -> Self {
        Self {
            obstacles,
            bounds,
            grid_resolution: 1.0,
        }
    }

    /// Generate a navmesh from obstacles within given bounds
    pub fn from_obstacles(obstacles: Vec<(Vec2, Vec2)>, bounds: (Vec2, Vec2)) -> Self {
        debug!(
            "Creating NavMesh with {} obstacles within bounds: {:?}",
            obstacles.len(),
            bounds
        );
        Self::new(obstacles, bounds)
    }

    /// Find a path from start to end using A* algorithm
    pub fn find_path(&self, start: Vec2, end: Vec2) -> Option<Vec<Vec2>> {
        // Check if both points are walkable
        if !self.is_walkable(start) || !self.is_walkable(end) {
            return None;
        }

        // Simple direct path if no obstacles block it
        if !self.line_intersects_obstacle(start, end) {
            return Some(vec![start, end]);
        }

        // Find waypoints through obstacles
        let mut path = vec![start];

        // Collect obstacle corners as potential waypoints
        let mut waypoints = Vec::new();
        for (min, max) in &self.obstacles {
            waypoints.push(*min);
            waypoints.push(Vec2::new(min.x, max.y));
            waypoints.push(Vec2::new(max.x, min.y));
            waypoints.push(*max);
        }

        // Simple pathfinding: try direct, then use waypoint if blocked
        let mut current = start;
        path.clear();
        path.push(start);

        while (current - end).length() > 0.1 {
            // Try direct path
            if !self.line_intersects_obstacle(current, end) {
                path.push(end);
                break;
            }

            // Find nearest valid waypoint
            let mut best_waypoint = None;
            let mut best_dist = f32::INFINITY;

            for &wp in &waypoints {
                if (wp - current).length() < 0.1 {
                    continue;
                }
                if (wp - end).length() < 0.1 {
                    continue;
                }
                if !self.line_intersects_obstacle(current, wp) {
                    let dist = (wp - end).length();
                    if dist < best_dist {
                        best_dist = dist;
                        best_waypoint = Some(wp);
                    }
                }
            }

            if let Some(wp) = best_waypoint {
                path.push(wp);
                current = wp;
            } else {
                // No valid waypoint found, return partial path
                break;
            }

            // Prevent infinite loops
            if path.len() > 20 {
                break;
            }
        }

        if path.len() > 1 {
            Some(path)
        } else {
            None
        }
    }

    /// Check if a position is walkable (not inside an obstacle)
    pub fn is_walkable(&self, position: Vec2) -> bool {
        // Check bounds
        if position.x < self.bounds.0.x
            || position.x > self.bounds.1.x
            || position.y < self.bounds.0.y
            || position.y > self.bounds.1.y
        {
            return false;
        }

        // Check obstacles
        for (min, max) in &self.obstacles {
            if position.x >= min.x && position.x <= max.x
                && position.y >= min.y && position.y <= max.y
            {
                return false;
            }
        }

        true
    }

    /// Find the nearest walkable position to a given position
    pub fn nearest_walkable(&self, position: Vec2) -> Vec2 {
        if self.is_walkable(position) {
            return position;
        }

        // Search in expanding circles
        for radius in (0..100).map(|i| (i as f32) * 0.5) {
            for angle_steps in 0..32 {
                let angle = (angle_steps as f32) * std::f32::consts::TAU / 32.0;
                let test_pos = position + Vec2::new(angle.cos(), angle.sin()) * radius;
                if self.is_walkable(test_pos) {
                    return test_pos;
                }
            }
        }

        // Fallback to bounds center
        (self.bounds.0 + self.bounds.1) * 0.5
    }

    /// Check if a line segment intersects any obstacle
    fn line_intersects_obstacle(&self, a: Vec2, b: Vec2) -> bool {
        for (min, max) in &self.obstacles {
            // Simple AABB-line segment intersection
            if self.line_intersects_aabb(a, b, *min, *max) {
                return true;
            }
        }
        false
    }

    /// Check if a line segment intersects an axis-aligned bounding box
    fn line_intersects_aabb(&self, a: Vec2, b: Vec2, min: Vec2, max: Vec2) -> bool {
        // Expand box slightly to avoid edge cases
        let expand = 0.1;
        let min = min - Vec2::splat(expand);
        let max = max + Vec2::splat(expand);

        // Check if either endpoint is inside the box
        if (a.x >= min.x && a.x <= max.x && a.y >= min.y && a.y <= max.y)
            || (b.x >= min.x && b.x <= max.x && b.y >= min.y && b.y <= max.y)
        {
            return true;
        }

        // Check if line segment intersects box edges
        let dx = b.x - a.x;
        let dy = b.y - a.y;

        if dx.abs() > 0.001 {
            let t1 = (min.x - a.x) / dx;
            let t2 = (max.x - a.x) / dx;
            let (tmin, tmax) = if t1 < t2 { (t1, t2) } else { (t2, t1) };

            if tmin < 1.0 && tmax > 0.0 {
                let ty1 = a.y + tmin.max(0.0) * dy;
                let ty2 = a.y + tmax.min(1.0) * dy;
                if (ty1 >= min.y && ty1 <= max.y) || (ty2 >= min.y && ty2 <= max.y) {
                    return true;
                }
            }
        }

        if dy.abs() > 0.001 {
            let t1 = (min.y - a.y) / dy;
            let t2 = (max.y - a.y) / dy;
            let (tmin, tmax) = if t1 < t2 { (t1, t2) } else { (t2, t1) };

            if tmin < 1.0 && tmax > 0.0 {
                let tx1 = a.x + tmin.max(0.0) * dx;
                let tx2 = a.x + tmax.min(1.0) * dx;
                if (tx1 >= min.x && tx1 <= max.x) || (tx2 >= min.x && tx2 <= max.x) {
                    return true;
                }
            }
        }

        false
    }
}

// ============================================================
// GRID PATHFINDER (A* BASED)
// ============================================================

/// Simple grid-based A* pathfinder
#[derive(Clone, Debug)]
pub struct GridPathfinder {
    width: usize,
    height: usize,
    cell_size: f32,
    blocked: Vec<bool>,
}

impl GridPathfinder {
    /// Create a new grid pathfinder
    pub fn new(width: usize, height: usize, cell_size: f32) -> Self {
        let cell_count = width.saturating_mul(height);
        Self {
            width,
            height,
            cell_size: cell_size.max(0.1),
            blocked: vec![false; cell_count],
        }
    }

    /// Mark a grid cell as blocked or walkable
    pub fn set_blocked(&mut self, x: usize, y: usize, blocked: bool) {
        if x < self.width && y < self.height {
            self.blocked[y * self.width + x] = blocked;
        }
    }

    /// Check if a cell is blocked
    pub fn is_blocked(&self, x: usize, y: usize) -> bool {
        if x >= self.width || y >= self.height {
            return true; // Out of bounds = blocked
        }
        self.blocked[y * self.width + x]
    }

    /// Convert world position to grid coordinates
    fn world_to_grid(&self, pos: Vec2) -> (usize, usize) {
        (
            (pos.x / self.cell_size) as usize,
            (pos.y / self.cell_size) as usize,
        )
    }

    /// Convert grid coordinates to world position (center of cell)
    fn grid_to_world(&self, x: usize, y: usize) -> Vec2 {
        Vec2::new(
            (x as f32 + 0.5) * self.cell_size,
            (y as f32 + 0.5) * self.cell_size,
        )
    }

    /// Find a path from start to end using A*
    pub fn find_path(&self, start: Vec2, end: Vec2) -> Option<Vec<Vec2>> {
        let (sx, sy) = self.world_to_grid(start);
        let (ex, ey) = self.world_to_grid(end);

        // Bounds check
        if sx >= self.width || sy >= self.height || ex >= self.width || ey >= self.height {
            return None;
        }

        // Check if start or end is blocked
        if self.is_blocked(sx, sy) || self.is_blocked(ex, ey) {
            return None;
        }

        // Use pathfinding crate's astar
        let result = pathfinding::prelude::astar(
            &(sx, sy),
            |&(x, y)| {
                let mut neighbors = Vec::new();
                // 4-directional movement
                for &(dx, dy) in &[(0, 1), (1, 0), (0, -1), (-1, 0)] {
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    if nx >= 0 && nx < self.width as i32 && ny >= 0 && ny < self.height as i32 {
                        let nx = nx as usize;
                        let ny = ny as usize;
                        if !self.is_blocked(nx, ny) {
                            neighbors.push(((nx, ny), 1));
                        }
                    }
                }
                neighbors
            },
            |&(x, y)| {
                let dx = (x as i32 - ex as i32).abs() as f32;
                let dy = (y as i32 - ey as i32).abs() as f32;
                (dx + dy) as u32 // Manhattan distance heuristic
            },
            |&pos| pos == (ex, ey),
        );

        result.map(|(path, _)| {
            path.into_iter()
                .map(|(x, y)| self.grid_to_world(x, y))
                .collect()
        })
    }

    /// Get the grid width
    pub fn get_width(&self) -> usize {
        self.width
    }

    /// Get the grid height
    pub fn get_height(&self) -> usize {
        self.height
    }

    /// Get the cell size
    pub fn get_cell_size(&self) -> f32 {
        self.cell_size
    }

    /// Clear all blocked cells
    pub fn clear(&mut self) {
        self.blocked.fill(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spatial_index_insert_query() {
        let mut index = SpatialIndex::new();
        index.insert(1, Vec2::new(0.0, 0.0), 1.0);
        index.insert(2, Vec2::new(5.0, 5.0), 1.0);

        let results = index.query_radius(Vec2::new(0.0, 0.0), 2.0);
        assert!(results.iter().any(|r| r.entity_id == 1));
        assert!(!results.iter().any(|r| r.entity_id == 2));
    }

    #[test]
    fn test_uniform_grid_neighbors() {
        let mut grid = UniformGrid::new(10.0);
        grid.insert(1, Vec2::new(5.0, 5.0));
        grid.insert(2, Vec2::new(15.0, 5.0));
        grid.insert(3, Vec2::new(25.0, 25.0));

        let neighbors = grid.query_neighbors(Vec2::new(12.0, 12.0));
        assert!(neighbors.contains(&1) || neighbors.contains(&2));
    }

    #[test]
    fn test_navmesh_walkable() {
        let obstacles = vec![(Vec2::new(5.0, 5.0), Vec2::new(10.0, 10.0))];
        let bounds = (Vec2::new(0.0, 0.0), Vec2::new(20.0, 20.0));
        let navmesh = NavMesh::from_obstacles(obstacles, bounds);

        assert!(navmesh.is_walkable(Vec2::new(0.0, 0.0)));
        assert!(!navmesh.is_walkable(Vec2::new(7.0, 7.0)));
    }

    #[test]
    fn test_grid_pathfinder_basic() {
        let pathfinder = GridPathfinder::new(10, 10, 1.0);
        let path = pathfinder.find_path(Vec2::new(0.5, 0.5), Vec2::new(9.5, 9.5));
        assert!(path.is_some());
    }
}

/// Initialize the spatial module (logging)
pub fn init() {
    log::info!(
        "{} initialized (spatial indexing, pathfinding, raycasting)",
        env!("CARGO_PKG_NAME")
    );
}
