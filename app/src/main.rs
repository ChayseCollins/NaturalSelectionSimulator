//! Natural Selection Simulator desktop application: the Week 1 view (M-10, M-11).
//!
//! The authoritative state is the `sim::Simulation` inside [`AppSession`]; every Bevy
//! entity, the food texture and the camera are projections of it or view-only state.
//! Frames feed Bevy's real (unclamped) frame time to the existing `SteppingDriver`, which
//! alone decides how many whole ticks run. Nothing here changes simulation state except
//! through the session's play, pause, reset and frame calls.

use std::time::Instant;

use app::camera::CameraView;
use app::driver::DriverLimits;
use app::render::{VisualMap, food_texture_len, write_food_pixels};
use app::session::{RUN_SEED, Session, SyncRequest, app_scenario};
use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::window::PrimaryWindow;
use bevy_egui::input::{egui_wants_any_keyboard_input, egui_wants_any_pointer_input};
use bevy_egui::{EguiContexts, EguiPlugin, EguiPrimaryContextPass, egui};

/// Background outside the world rectangle.
const BACKGROUND: Color = Color::srgb(0.07, 0.08, 0.11);
/// Prey markers: one bright color, readable over every food shade.
const PREY_COLOR: Color = Color::srgb(1.0, 0.62, 0.16);
/// Prey marker side in world units (a cell and a half).
const PREY_MARKER_SIZE: f32 = 3.0;
/// Keyboard pan speed in logical pixels per second, so it feels the same at every zoom.
const KEY_PAN_PIXELS_PER_SECOND: f32 = 600.0;
/// Wheel distance in pixels treated as one zoom step (touchpads report pixels).
const PIXELS_PER_SCROLL_STEP: f32 = 40.0;
const WINDOW_WIDTH: u32 = 1280;
const WINDOW_HEIGHT: u32 = 720;

#[derive(Resource)]
struct AppSession(Session);

#[derive(Resource)]
struct FoodTexture(Handle<Image>);

#[derive(Resource, Default)]
struct PreyVisuals(VisualMap<Entity>);

#[derive(Resource)]
struct WorldCamera(CameraView);

#[derive(Component)]
struct PreyMarker;

fn main() -> AppExit {
    let session = match Session::new(app_scenario(), RUN_SEED, DriverLimits::default()) {
        Ok(session) => session,
        Err(errors) => {
            for error in errors {
                eprintln!("error: invalid scenario: {error}");
            }
            return AppExit::error();
        }
    };
    App::new()
        .insert_resource(ClearColor(BACKGROUND))
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Natural Selection Simulator".into(),
                resolution: (WINDOW_WIDTH, WINDOW_HEIGHT).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(EguiPlugin::default())
        .insert_resource(AppSession(session))
        .init_resource::<PreyVisuals>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                (
                    pointer_camera.run_if(not(egui_wants_any_pointer_input)),
                    keyboard_camera.run_if(not(egui_wants_any_keyboard_input)),
                    apply_camera,
                )
                    .chain(),
                (step_simulation, sync_visuals).chain(),
            ),
        )
        .add_systems(EguiPrimaryContextPass, control_panel)
        .run()
}

/// Spawns the camera, fitted to the world, and the food texture sprite. Prey sprites are
/// created by the first `sync_visuals`.
fn setup(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    session: Res<AppSession>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
) {
    let simulation = session.0.simulation();
    let world = &simulation.config().world;
    let food = simulation.food();

    let mut image = Image::new_fill(
        Extent3d {
            width: food.width(),
            height: food.height(),
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 255],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::nearest();
    if let Some(pixels) = image.data.as_mut()
        && pixels.len() == food_texture_len(food)
    {
        write_food_pixels(food, pixels);
    }
    let texture = images.add(image);

    // The grid can extend past the world edge when the world is not a whole number of
    // cells; draw it at its true size from the world origin.
    let grid_width = food.width() as f32 * food.cell_size();
    let grid_height = food.height() as f32 * food.cell_size();
    commands.spawn((
        Sprite {
            image: texture.clone(),
            custom_size: Some(Vec2::new(grid_width, grid_height)),
            ..default()
        },
        Transform::from_xyz(grid_width * 0.5, grid_height * 0.5, 0.0),
    ));
    commands.insert_resource(FoodTexture(texture));

    let (viewport_width, viewport_height) = window
        .map(|window| (window.width(), window.height()))
        .unwrap_or((WINDOW_WIDTH as f32, WINDOW_HEIGHT as f32));
    let view = CameraView::fit(world.width, world.height, viewport_width, viewport_height);
    commands.spawn((
        Camera2d,
        Transform::from_xyz(view.center_x, view.center_y, 0.0),
        Projection::Orthographic(OrthographicProjection {
            scale: view.scale,
            ..OrthographicProjection::default_2d()
        }),
    ));
    commands.insert_resource(WorldCamera(view));
}

/// Hands this frame's real elapsed time to the driver. The monotonic work timer lives
/// here, in the app; `sim` never reads a clock.
fn step_simulation(time: Res<Time<Real>>, mut session: ResMut<AppSession>) {
    let work_started = Instant::now();
    session.0.frame(time.delta(), || work_started.elapsed());
}

/// Refreshes the food texture and prey sprites after ticks ran, or rebuilds them after a
/// start or reset. Reads simulation state only.
fn sync_visuals(
    mut commands: Commands,
    mut session: ResMut<AppSession>,
    food_texture: Res<FoodTexture>,
    mut images: ResMut<Assets<Image>>,
    mut prey: ResMut<PreyVisuals>,
    mut markers: Query<&mut Transform, With<PreyMarker>>,
) {
    let request = session.0.take_sync();
    if request == SyncRequest::None {
        return;
    }
    let simulation = session.0.simulation();

    if let Some(mut image) = images.get_mut(&food_texture.0)
        && let Some(pixels) = image.data.as_mut()
    {
        write_food_pixels(simulation.food(), pixels);
    }

    if request == SyncRequest::Rebuild {
        for entity in prey.0.drain() {
            commands.entity(entity).despawn();
        }
    }
    let organisms = simulation.organisms();
    let plan = prey.0.plan(organisms.ids());
    for entity in plan.removed {
        commands.entity(entity).despawn();
    }
    for index in plan.missing {
        let entity = commands
            .spawn((
                PreyMarker,
                Sprite::from_color(PREY_COLOR, Vec2::splat(PREY_MARKER_SIZE)),
                Transform::from_xyz(organisms.x()[index], organisms.y()[index], 1.0),
            ))
            .id();
        prey.0.insert(organisms.ids()[index], entity);
    }
    for (index, entity) in plan.existing {
        if let Ok(mut transform) = markers.get_mut(entity) {
            transform.translation.x = organisms.x()[index];
            transform.translation.y = organisms.y()[index];
        }
    }
}

/// Right- or middle-button drag pans; the wheel zooms about the view center.
fn pointer_camera(
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    session: Res<AppSession>,
    mut camera: ResMut<WorldCamera>,
) {
    let world = &session.0.simulation().config().world;
    if buttons.any_pressed([MouseButton::Right, MouseButton::Middle]) && motion.delta != Vec2::ZERO
    {
        camera
            .0
            .drag(motion.delta.x, motion.delta.y, world.width, world.height);
    }
    let steps = match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y,
        MouseScrollUnit::Pixel => scroll.delta.y / PIXELS_PER_SCROLL_STEP,
    };
    camera.0.zoom_steps(steps);
}

/// WASD or arrow keys pan at a constant on-screen speed.
fn keyboard_camera(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time<Real>>,
    session: Res<AppSession>,
    mut camera: ResMut<WorldCamera>,
) {
    let mut direction = Vec2::ZERO;
    if keys.any_pressed([KeyCode::KeyA, KeyCode::ArrowLeft]) {
        direction.x -= 1.0;
    }
    if keys.any_pressed([KeyCode::KeyD, KeyCode::ArrowRight]) {
        direction.x += 1.0;
    }
    if keys.any_pressed([KeyCode::KeyS, KeyCode::ArrowDown]) {
        direction.y -= 1.0;
    }
    if keys.any_pressed([KeyCode::KeyW, KeyCode::ArrowUp]) {
        direction.y += 1.0;
    }
    if direction == Vec2::ZERO {
        return;
    }
    let world = &session.0.simulation().config().world;
    let distance = KEY_PAN_PIXELS_PER_SECOND * time.delta_secs();
    camera.0.pan_screen(
        direction.x * distance,
        direction.y * distance,
        world.width,
        world.height,
    );
}

/// Copies the camera view into the Bevy camera.
fn apply_camera(
    camera: Res<WorldCamera>,
    view: Option<Single<(&mut Transform, &mut Projection), With<Camera2d>>>,
) {
    let Some(mut view) = view else {
        return;
    };
    let (ref mut transform, ref mut projection) = *view;
    transform.translation.x = camera.0.center_x;
    transform.translation.y = camera.0.center_y;
    if let Projection::Orthographic(orthographic) = projection.as_mut() {
        orthographic.scale = camera.0.scale;
    }
}

/// Compact control and status panel: Play, Pause, Reset and the run's readouts.
fn control_panel(mut contexts: EguiContexts, mut session: ResMut<AppSession>) -> Result {
    let ctx = contexts.ctx_mut()?;
    let session = &mut session.0;
    egui::Window::new("Simulation")
        .default_pos([12.0, 12.0])
        .resizable(false)
        .show(ctx, |ui| {
            let paused = session.driver().is_paused();
            ui.horizontal(|ui| {
                if ui.add_enabled(paused, egui::Button::new("Play")).clicked() {
                    session.play();
                }
                if ui
                    .add_enabled(!paused, egui::Button::new("Pause"))
                    .clicked()
                {
                    session.pause();
                }
                if ui.button("Reset").clicked()
                    && let Err(errors) = session.reset()
                {
                    for error in errors {
                        error!("reset failed: {error}");
                    }
                }
            });

            let simulation = session.simulation();
            let driver = session.driver();
            let summary = simulation.summary_metrics();
            ui.label(if driver.is_paused() {
                "Paused"
            } else {
                "Running at 1×"
            });
            ui.label(format!("Seed {}", session.run_seed()));
            ui.label(format!("Tick {}", summary.tick));
            ui.label(format!("Simulated time {:.1} s", summary.sim_seconds));
            ui.label(format!("Prey {}", summary.prey_population));
            ui.label(format!(
                "Starvation deaths {}",
                summary.cumulative_starvation_deaths
            ));
            ui.label(format!(
                "Food consumed {:.1}",
                summary.cumulative_food_consumed
            ));
            ui.monospace(format!("Hash 0x{:016x}", summary.state_hash));
            if let Some(speed) = driver.achieved_speed() {
                ui.label(format!("Achieved speed {speed:.2}×"));
            }
            if driver.is_behind() {
                ui.colored_label(
                    egui::Color32::YELLOW,
                    format!(
                        "Behind: {} ticks ({:.2} s) backlog",
                        driver.backlog_ticks(),
                        driver.backlog_sim_seconds()
                    ),
                );
            }
            if driver.stall_count() > 0 {
                ui.label(format!("Stall-capped frames {}", driver.stall_count()));
            }
            ui.separator();
            ui.small("Pan: right- or middle-drag, WASD or arrow keys");
            ui.small("Zoom: mouse wheel");
        });
    Ok(())
}
