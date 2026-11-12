//! World-camera math for the Week 1 view (M-10): fit, pan and zoom.
//!
//! Pure state with no Bevy types, so it is testable without a window. The camera only
//! changes what is shown: it never touches the simulation, its random domains or its
//! state hash.
//!
//! `scale` is world units per logical pixel (Bevy's orthographic `scale` under its
//! default window-size scaling). World coordinates match `sim`'s: x right, y up, the
//! world occupying `[0, width] × [0, height]`.

/// Most zoomed in: 0.01 world units per pixel, so 100 pixels per world unit (P).
pub const MIN_SCALE: f32 = 0.01;
/// Most zoomed out: 16 world units per pixel (P). This shows even the largest allowed
/// world (4,096 units) in under 300 pixels.
pub const MAX_SCALE: f32 = 16.0;
/// Scale change per wheel step (P).
pub const ZOOM_STEP: f32 = 1.1;
/// Extra room around the world when fitting the view (P).
pub const FIT_MARGIN: f32 = 1.05;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraView {
    pub center_x: f32,
    pub center_y: f32,
    /// World units per logical pixel; always within `[MIN_SCALE, MAX_SCALE]`.
    pub scale: f32,
}

impl CameraView {
    /// View centered on the world with all of it visible in a viewport of the given size
    /// (logical pixels). Degenerate viewport sizes are treated as 1 pixel.
    pub fn fit(
        world_width: f32,
        world_height: f32,
        viewport_width: f32,
        viewport_height: f32,
    ) -> Self {
        let viewport_width = positive_or_one(viewport_width);
        let viewport_height = positive_or_one(viewport_height);
        let scale = (world_width / viewport_width).max(world_height / viewport_height) * FIT_MARGIN;
        Self {
            center_x: world_width * 0.5,
            center_y: world_height * 0.5,
            scale: clamp_scale(scale),
        }
    }

    /// Moves the view by a screen-space amount in logical pixels, x right and y up, then
    /// keeps the view center inside the world so the map cannot be lost. Non-finite input
    /// is ignored.
    pub fn pan_screen(&mut self, dx: f32, dy: f32, world_width: f32, world_height: f32) {
        if !dx.is_finite() || !dy.is_finite() {
            return;
        }
        self.center_x = (self.center_x + dx * self.scale).clamp(0.0, world_width);
        self.center_y = (self.center_y + dy * self.scale).clamp(0.0, world_height);
    }

    /// Mouse drag by `(dx, dy)` logical pixels in window coordinates (y down): the world
    /// follows the pointer.
    pub fn drag(&mut self, dx: f32, dy: f32, world_width: f32, world_height: f32) {
        self.pan_screen(-dx, dy, world_width, world_height);
    }

    /// Zooms about the view center: positive steps zoom in. The scale stays within
    /// `[MIN_SCALE, MAX_SCALE]`, so the view never collapses or inverts. Non-finite input
    /// is ignored.
    pub fn zoom_steps(&mut self, steps: f32) {
        if !steps.is_finite() || steps == 0.0 {
            return;
        }
        self.scale = clamp_scale(self.scale * ZOOM_STEP.powf(-steps));
    }
}

fn positive_or_one(value: f32) -> f32 {
    if value.is_finite() && value >= 1.0 {
        value
    } else {
        1.0
    }
}

fn clamp_scale(scale: f32) -> f32 {
    if scale.is_finite() {
        scale.clamp(MIN_SCALE, MAX_SCALE)
    } else {
        MAX_SCALE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_view() -> CameraView {
        CameraView::fit(512.0, 512.0, 1280.0, 720.0)
    }

    fn assert_valid(view: &CameraView) {
        assert!(
            view.center_x.is_finite() && view.center_y.is_finite(),
            "{view:?}"
        );
        assert!((MIN_SCALE..=MAX_SCALE).contains(&view.scale), "{view:?}");
    }

    #[test]
    fn fit_shows_the_whole_world_centered() {
        let view = default_view();
        assert_eq!((view.center_x, view.center_y), (256.0, 256.0));
        // The taller dimension binds: 512 units in 720 pixels, plus the margin.
        assert!((view.scale - 512.0 / 720.0 * FIT_MARGIN).abs() < 1e-6);
        assert!(view.scale * 1280.0 >= 512.0 && view.scale * 720.0 >= 512.0);
        assert_valid(&CameraView::fit(512.0, 512.0, 0.0, f32::NAN));
    }

    #[test]
    fn zoom_is_clamped_and_never_inverts() {
        let mut view = default_view();
        let start = view.scale;
        view.zoom_steps(1.0);
        assert!(view.scale < start, "positive steps zoom in");
        view.zoom_steps(-2.0);
        assert!(view.scale > start, "negative steps zoom out");
        view.zoom_steps(10_000.0);
        assert_eq!(view.scale, MIN_SCALE);
        view.zoom_steps(-10_000.0);
        assert_eq!(view.scale, MAX_SCALE);
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            view.zoom_steps(bad);
            assert_valid(&view);
        }
        assert!(view.scale > 0.0);
    }

    #[test]
    fn pan_moves_the_view_and_keeps_its_center_in_the_world() {
        let mut view = default_view();
        let scale = view.scale;
        view.pan_screen(10.0, -20.0, 512.0, 512.0);
        assert!((view.center_x - (256.0 + 10.0 * scale)).abs() < 1e-4);
        assert!((view.center_y - (256.0 - 20.0 * scale)).abs() < 1e-4);

        let before = view;
        view.drag(10.0, 20.0, 512.0, 512.0);
        assert!(
            view.center_x < before.center_x,
            "dragging right moves the view left"
        );
        assert!(
            view.center_y > before.center_y,
            "dragging down moves the view up"
        );

        view.pan_screen(1e9, -1e9, 512.0, 512.0);
        assert_eq!(
            (view.center_x, view.center_y),
            (512.0, 0.0),
            "clamped to the world"
        );
        view.pan_screen(f32::NAN, 1.0, 512.0, 512.0);
        assert_valid(&view);
    }
}
