//! 2D camera with smooth following and coordinate transformations

use glam::{Mat3, Vec2};

/// 2D camera for viewport management
#[derive(Debug, Clone)]
pub struct Camera {
    /// World position of camera center
    pub position: Vec2,
    /// Zoom level (1.0 = 100%, 0.5 = 50%, 2.0 = 200%)
    pub zoom: f32,
    /// Rotation in radians (usually 0 for top-down games)
    pub rotation: f32,
    /// Viewport dimensions in pixels
    pub viewport_width: f32,
    pub viewport_height: f32,
    /// Smoothing factor for follow (0.0-1.0, higher = faster)
    pub follow_smoothness: f32,
    /// Target position for smooth following
    follow_target: Option<Vec2>,
}

impl Camera {
    /// Create a new camera at the given position
    pub fn new(position: Vec2, width: f32, height: f32) -> Self {
        Self {
            position,
            zoom: 1.0,
            rotation: 0.0,
            viewport_width: width,
            viewport_height: height,
            follow_smoothness: 0.1,
            follow_target: None,
        }
    }

    /// Create a default camera (origin, 1280x720)
    pub fn default() -> Self {
        Self::new(Vec2::ZERO, 1280.0, 720.0)
    }

    /// Set follow target (use None to stop following)
    pub fn set_follow_target(&mut self, target: Option<Vec2>) {
        self.follow_target = target;
    }

    /// Update camera to smoothly follow target (call once per frame)
    pub fn update(&mut self, dt: f32) {
        if let Some(target) = self.follow_target {
            let direction = target - self.position;
            let distance = direction.length();

            if distance > 0.01 {
                // Smooth interpolation
                let step = self.follow_smoothness * dt;
                self.position += direction.normalize() * (distance * step).min(distance);
            }
        }
    }

    /// Convert world coordinates to screen coordinates
    pub fn world_to_screen(&self, world_pos: Vec2) -> Vec2 {
        // Translate by camera position
        let relative = world_pos - self.position;

        // Apply rotation
        let rotated = if self.rotation.abs() > 0.001 {
            let cos_r = self.rotation.cos();
            let sin_r = self.rotation.sin();
            Vec2::new(
                relative.x * cos_r - relative.y * sin_r,
                relative.x * sin_r + relative.y * cos_r,
            )
        } else {
            relative
        };

        // Apply zoom and translate to screen center
        let scaled = rotated * self.zoom;
        Vec2::new(
            self.viewport_width / 2.0 + scaled.x,
            self.viewport_height / 2.0 - scaled.y, // Flip Y for screen coordinates
        )
    }

    /// Convert screen coordinates to world coordinates
    pub fn screen_to_world(&self, screen_pos: Vec2) -> Vec2 {
        // Translate from screen center
        let centered = Vec2::new(
            screen_pos.x - self.viewport_width / 2.0,
            self.viewport_height / 2.0 - screen_pos.y, // Flip Y back
        );

        // Inverse zoom
        let unscaled = centered / self.zoom;

        // Inverse rotation
        let unrotated = if self.rotation.abs() > 0.001 {
            let cos_r = self.rotation.cos();
            let sin_r = self.rotation.sin();
            Vec2::new(
                unscaled.x * cos_r + unscaled.y * sin_r,
                -unscaled.x * sin_r + unscaled.y * cos_r,
            )
        } else {
            unscaled
        };

        // Translate by camera position
        self.position + unrotated
    }

    /// Get the world-space bounds of the viewport
    pub fn viewport_bounds(&self) -> (Vec2, Vec2) {
        let half_width = self.viewport_width / (2.0 * self.zoom);
        let half_height = self.viewport_height / (2.0 * self.zoom);

        let min = self.position - Vec2::new(half_width, half_height);
        let max = self.position + Vec2::new(half_width, half_height);

        (min, max)
    }

    /// Create projection matrix for rendering
    pub fn projection_matrix(&self) -> Mat3 {
        // Orthographic projection with camera transform
        let width_inv = 2.0 / self.viewport_width;
        let height_inv = 2.0 / self.viewport_height;

        // Start with orthographic projection
        let mut proj = Mat3::from_scale(Vec2::new(width_inv * self.zoom, height_inv * self.zoom));

        // Apply camera position offset
        proj.z_axis.x = -self.position.x * width_inv * self.zoom;
        proj.z_axis.y = self.position.y * height_inv * self.zoom;

        proj
    }

    /// Resize viewport
    pub fn set_viewport(&mut self, width: f32, height: f32) {
        self.viewport_width = width;
        self.viewport_height = height;
    }

    /// Set zoom level
    pub fn set_zoom(&mut self, zoom: f32) {
        self.zoom = zoom.max(0.1);
    }

    /// Adjust zoom (positive = zoom in, negative = zoom out)
    pub fn adjust_zoom(&mut self, delta: f32) {
        self.zoom = (self.zoom + delta).max(0.1);
    }

    /// Set rotation in radians
    pub fn set_rotation(&mut self, rotation: f32) {
        self.rotation = rotation;
    }

    /// Instantly move to position
    pub fn set_position(&mut self, position: Vec2) {
        self.position = position;
        self.follow_target = None; // Stop following
    }

    /// Shake camera (useful for impacts)
    pub fn shake(&mut self, amount: Vec2) {
        self.position += amount;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_world_to_screen_identity() {
        let camera = Camera::new(Vec2::ZERO, 1280.0, 720.0);
        let screen = camera.world_to_screen(Vec2::ZERO);
        assert!((screen.x - 640.0).abs() < 0.01);
        assert!((screen.y - 360.0).abs() < 0.01);
    }

    #[test]
    fn test_screen_to_world_identity() {
        let camera = Camera::new(Vec2::ZERO, 1280.0, 720.0);
        let world = camera.screen_to_world(Vec2::new(640.0, 360.0));
        assert!(world.length() < 0.01);
    }

    #[test]
    fn test_round_trip() {
        let camera = Camera::new(Vec2::new(100.0, 200.0), 1280.0, 720.0);
        let original = Vec2::new(500.0, 300.0);
        let screen = camera.world_to_screen(original);
        let world = camera.screen_to_world(screen);
        let delta = (world - original).length();
        assert!(delta < 0.1, "Round-trip error: {}", delta);
    }
}
