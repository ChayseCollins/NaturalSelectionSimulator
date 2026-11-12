//! Natural Selection Simulator desktop application (M-10 to M-16, M-18).
//!
//! The authoritative state is the `sim::Simulation` inside the [`Session`]; every Bevy
//! entity, texture and the camera are projections of it or view-only state. Frames feed
//! Bevy's real (unclamped) frame time to the `SteppingDriver`, which alone decides how many
//! whole ticks run. Nothing here changes simulation state except through the session's
//! play, pause, step, reset and load calls.
//!
//! Launch: no arguments opens the baseline preset paused at tick 0;
//! `app --scenario <file> [--seed <n>]` opens a scenario file (SCOPE 8). For documentation
//! and visual checks, `--screenshot <file.png> [--after <seconds>] [--play] [--speed <x>]
//! [--select <id>]` saves one picture of the window after the given real time and exits.

// Bevy systems take their resources and queries as parameters, so long signatures and
// nested query filters are normal; Bevy recommends allowing these two lints for them.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

use std::path::PathBuf;
use std::time::{Duration, Instant};

use app::camera::CameraView;
use app::driver::{DriverLimits, Speed};
use app::perf::PerfWindow;
use app::presets;
use app::render::{self, SHAPE_PIXELS, Shape};
use app::session::{Session, SyncRequest};
use app::ui::{self, UiState};
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{CameraOutputMode, Viewport};
use bevy::image::ImageSampler;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;
use bevy::render::render_resource::{BlendState, Extent3d, TextureDimension, TextureFormat};
use bevy::render::view::window::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use bevy::window::PrimaryWindow;
use bevy_egui::input::{egui_wants_any_keyboard_input, egui_wants_any_pointer_input};
use bevy_egui::{
    EguiContexts, EguiGlobalSettings, EguiPlugin, EguiPrimaryContextPass, PrimaryEguiContext, egui,
};
use sim::{Species, TraitId};

/// Background outside the world rectangle.
const BACKGROUND: Color = Color::srgb(0.06, 0.07, 0.09);
/// Keyboard pan speed in logical pixels per second, so it feels the same at every zoom.
const KEY_PAN_PIXELS_PER_SECOND: f32 = 700.0;
/// Wheel distance in pixels treated as one zoom step (touchpads report pixels).
const PIXELS_PER_SCROLL_STEP: f32 = 40.0;
/// The food overlay is redrawn every this many frames while running (SCOPE 7.3: 15, P).
const FOOD_REFRESH_FRAMES: u64 = 15;
/// Click picking radius in logical pixels.
const PICK_RADIUS_PIXELS: f32 = 10.0;
/// SCOPE 10.2 measurement window size.
const WINDOW_WIDTH: u32 = 1600;
const WINDOW_HEIGHT: u32 = 900;

#[derive(Resource)]
struct AppSession(Session);

#[derive(Resource)]
struct Ui(UiState);

#[derive(Resource)]
struct WorldCamera(CameraView);

/// The world viewport: the window area left free by the panels, in logical pixels.
#[derive(Resource, Clone, Copy)]
struct ViewportArea {
    min: Vec2,
    size: Vec2,
}

#[derive(Resource)]
struct Textures {
    world: Handle<Image>,
    disc: Handle<Image>,
    triangle: Handle<Image>,
    ring: Handle<Image>,
}

/// Sprite pool: entity `k` shows the organism at storage index `k` (SCOPE 7.3: a pool
/// reused across births and deaths, positioned each frame from `sim`'s slices).
#[derive(Resource, Default)]
struct Pool {
    entities: Vec<Entity>,
    /// What the pool last showed: tick, color mode, organism count.
    shown: Option<(u64, render::ColorBy, usize)>,
}

#[derive(Resource, Default)]
struct FrameStats {
    frames: u64,
    window: PerfWindow,
    last_sim: Duration,
    last_ticks: u64,
}

#[derive(Component)]
struct WorldView;

#[derive(Component)]
struct WorldSprite;

#[derive(Component)]
struct OrganismSprite;

#[derive(Component)]
struct SelectionMarker;

const USAGE: &str = "usage: app [--scenario <file.toml>] [--seed <n>] [--screenshot <file.png> \
                     [--after <seconds>] [--play] [--speed 1|2|5|10|max] [--select <id>]]";

/// Parsed launch options.
struct Launch {
    scenario: Option<PathBuf>,
    seed: Option<u64>,
    screenshot: Option<PathBuf>,
    after_seconds: f32,
    play: bool,
    speed: Option<Speed>,
    select: Option<u64>,
}

fn parse_args() -> Result<Launch, String> {
    let mut launch = Launch {
        scenario: None,
        seed: None,
        screenshot: None,
        after_seconds: 8.0,
        play: false,
        speed: None,
        select: None,
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = |name: &str| args.next().ok_or(format!("{name} needs a value; {USAGE}"));
        match arg.as_str() {
            "--scenario" => launch.scenario = Some(PathBuf::from(value("--scenario")?)),
            "--seed" => {
                let text = value("--seed")?;
                launch.seed = Some(
                    text.parse()
                        .map_err(|_| format!("--seed: {text} is not a whole number"))?,
                );
            }
            "--screenshot" => launch.screenshot = Some(PathBuf::from(value("--screenshot")?)),
            "--after" => {
                let text = value("--after")?;
                launch.after_seconds = text
                    .parse::<f32>()
                    .ok()
                    .filter(|s| s.is_finite() && *s >= 0.0)
                    .ok_or(format!("--after: {text} is not a number of seconds"))?;
            }
            "--play" => launch.play = true,
            "--speed" => {
                let text = value("--speed")?;
                launch.speed = Some(match text.as_str() {
                    "1" => Speed::X1,
                    "2" => Speed::X2,
                    "5" => Speed::X5,
                    "10" => Speed::X10,
                    "max" => Speed::Max,
                    _ => return Err(format!("--speed: {text} is not 1, 2, 5, 10 or max")),
                });
            }
            "--select" => {
                let text = value("--select")?;
                launch.select = Some(
                    text.parse()
                        .map_err(|_| format!("--select: {text} is not an organism ID"))?,
                );
            }
            "-h" | "--help" => return Err(USAGE.to_string()),
            other => return Err(format!("unknown option {other}; {USAGE}")),
        }
    }
    Ok(launch)
}

/// A pending window screenshot (`--screenshot`): taken once `after` of real time has
/// passed, then the app exits.
#[derive(Resource)]
struct ScreenshotPlan {
    path: PathBuf,
    after: Duration,
    elapsed: Duration,
    requested: bool,
}

/// Saves one screenshot of the window once the plan's time has passed, then exits.
fn take_screenshot(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut plan: ResMut<ScreenshotPlan>,
) {
    plan.elapsed += time.delta();
    if plan.requested || plan.elapsed < plan.after {
        return;
    }
    plan.requested = true;
    let path = plan.path.clone();
    let mut save = save_to_disk(path.clone());
    commands.spawn(Screenshot::primary_window()).observe(
        move |captured: On<ScreenshotCaptured>| {
            save(captured);
            println!("screenshot saved to {}", path.display());
            std::process::exit(0);
        },
    );
}

fn main() -> AppExit {
    let launch = match parse_args() {
        Ok(launch) => launch,
        Err(message) => {
            eprintln!("{message}");
            return AppExit::error();
        }
    };
    let (presets, preset_errors) = presets::all();
    for error in &preset_errors {
        eprintln!("warning: preset skipped: {error}");
    }
    if presets.is_empty() {
        eprintln!("error: no valid preset is embedded");
        return AppExit::error();
    }
    let mut state = UiState::new(presets);
    let (config, source) = match &launch.scenario {
        None => (
            state.presets[0].config.clone(),
            state.presets[0].file.to_string(),
        ),
        Some(path) => match ui::scenarios::read_scenario(path) {
            Ok(config) => {
                state.preset_index = None;
                state.scenario_path = path.display().to_string();
                (config, path.display().to_string())
            }
            Err(message) => {
                eprintln!("error: {message}");
                return AppExit::error();
            }
        },
    };
    let seed = launch
        .seed
        .or(config.seed)
        .unwrap_or_else(ui::controls::fresh_seed);
    let mut session = match Session::new(config, seed, DriverLimits::default()) {
        Ok(session) => session,
        Err(errors) => {
            for error in errors {
                eprintln!("error: invalid scenario: {error}");
            }
            return AppExit::error();
        }
    };
    session.set_source(source);
    if let Some(speed) = launch.speed {
        session.set_speed(speed);
    }
    if launch.play {
        session.play();
    }
    state.selected = launch.select;

    let mut app = App::new();
    if let Some(path) = launch.screenshot {
        app.insert_resource(ScreenshotPlan {
            path,
            after: Duration::from_secs_f32(launch.after_seconds),
            elapsed: Duration::ZERO,
            requested: false,
        })
        .add_systems(Update, take_screenshot);
    }
    app.insert_resource(ClearColor(BACKGROUND))
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
        .insert_resource(Ui(state))
        .insert_resource(ViewportArea {
            min: Vec2::ZERO,
            size: Vec2::new(WINDOW_WIDTH as f32, WINDOW_HEIGHT as f32),
        })
        .init_resource::<Pool>()
        .init_resource::<FrameStats>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                keyboard_input.run_if(not(egui_wants_any_keyboard_input)),
                pointer_input.run_if(not(egui_wants_any_pointer_input)),
                step_simulation,
                sync_world_texture,
                sync_organisms,
                apply_camera,
            )
                .chain(),
        )
        .add_systems(EguiPrimaryContextPass, ui_system)
        .run()
}

fn shape_image(shape: Shape) -> Image {
    let mut image = Image::new(
        Extent3d {
            width: SHAPE_PIXELS,
            height: SHAPE_PIXELS,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        render::shape_pixels(shape),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::linear();
    image
}

/// An image with one pixel per terrain cell, filled from the simulation.
fn world_image(session: &Session, show_food: bool) -> Image {
    let s = session.simulation();
    let food = s.food();
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
    if let Some(pixels) = image.data.as_mut() {
        render::write_world_pixels(s.terrain(), food, show_food, pixels);
    }
    image
}

/// Size of the drawn world grid (the grid can extend past a world that is not a whole
/// number of cells; it is drawn at its true size from the origin).
fn grid_extent(session: &Session) -> Vec2 {
    let food = session.simulation().food();
    Vec2::new(
        food.width() as f32 * food.cell_size(),
        food.height() as f32 * food.cell_size(),
    )
}

fn setup(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut egui_settings: ResMut<EguiGlobalSettings>,
    session: Res<AppSession>,
    ui_state: Res<Ui>,
) {
    // The world camera gets a viewport between the panels, so egui renders through its
    // own full-window camera (bevy_egui's side-panel pattern).
    egui_settings.auto_create_primary_context = false;
    let world = &session.0.simulation().config().world;
    let view = CameraView::fit(
        world.width,
        world.height,
        WINDOW_WIDTH as f32,
        WINDOW_HEIGHT as f32,
    );
    commands.spawn((
        Camera2d,
        WorldView,
        Transform::from_xyz(view.center_x, view.center_y, 0.0),
        Projection::Orthographic(OrthographicProjection {
            scale: view.scale,
            ..OrthographicProjection::default_2d()
        }),
    ));
    commands.spawn((
        PrimaryEguiContext,
        Camera2d,
        RenderLayers::none(),
        Camera {
            order: 1,
            output_mode: CameraOutputMode::Write {
                blend_state: Some(BlendState::ALPHA_BLENDING),
                clear_color: ClearColorConfig::None,
            },
            clear_color: ClearColorConfig::Custom(Color::NONE),
            ..default()
        },
    ));
    commands.insert_resource(WorldCamera(view));

    let world_texture = images.add(world_image(&session.0, ui_state.0.show_food));
    let extent = grid_extent(&session.0);
    commands.spawn((
        WorldSprite,
        Sprite {
            image: world_texture.clone(),
            custom_size: Some(extent),
            ..default()
        },
        Transform::from_xyz(extent.x * 0.5, extent.y * 0.5, 0.0),
    ));
    let textures = Textures {
        world: world_texture,
        disc: images.add(shape_image(Shape::Disc)),
        triangle: images.add(shape_image(Shape::Triangle)),
        ring: images.add(shape_image(Shape::Ring)),
    };
    commands.spawn((
        SelectionMarker,
        Sprite {
            image: textures.ring.clone(),
            color: Color::WHITE,
            custom_size: Some(Vec2::splat(4.0)),
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, 5.0),
        Visibility::Hidden,
    ));
    commands.insert_resource(textures);
}

/// Keyboard shortcuts: Space play/pause, period (.) step, Home fit, Esc clear selection,
/// F3 overlay, WASD or arrows pan, +/- zoom.
fn keyboard_input(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time<Real>>,
    mut session: ResMut<AppSession>,
    mut ui_state: ResMut<Ui>,
    mut camera: ResMut<WorldCamera>,
) {
    let state = &mut ui_state.0;
    if keys.just_pressed(KeyCode::Space) {
        session.0.toggle_play();
    }
    if keys.just_pressed(KeyCode::Period) {
        session.0.step_once();
    }
    if keys.just_pressed(KeyCode::Home) {
        state.fit_requested = true;
    }
    if keys.just_pressed(KeyCode::Escape) {
        state.selected = None;
        state.follow_selected = false;
    }
    if keys.just_pressed(KeyCode::F3) {
        state.show_overlay = !state.show_overlay;
    }
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
    let world = &session.0.simulation().config().world;
    if direction != Vec2::ZERO {
        let distance = KEY_PAN_PIXELS_PER_SECOND * time.delta_secs();
        camera.0.pan_screen(
            direction.x * distance,
            direction.y * distance,
            world.width,
            world.height,
        );
        state.follow_selected = false;
    }
    if keys.any_pressed([KeyCode::Equal, KeyCode::NumpadAdd]) {
        camera.0.zoom_steps(time.delta_secs() * 8.0);
    }
    if keys.any_pressed([KeyCode::Minus, KeyCode::NumpadSubtract]) {
        camera.0.zoom_steps(-time.delta_secs() * 8.0);
    }
}

/// Mouse: right- or middle-drag pans, the wheel zooms toward the cursor, a left click
/// selects the nearest organism (M-12 picking through the spatial grid).
fn pointer_input(
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    area: Res<ViewportArea>,
    session: Res<AppSession>,
    mut ui_state: ResMut<Ui>,
    mut camera: ResMut<WorldCamera>,
) {
    let world = &session.0.simulation().config().world;
    let cursor = window
        .and_then(|window| window.cursor_position())
        .map(|position| position - area.min)
        .filter(|p| p.x >= 0.0 && p.y >= 0.0 && p.x <= area.size.x && p.y <= area.size.y);
    let Some(cursor) = cursor else {
        return;
    };
    let viewport = (area.size.x, area.size.y);
    if buttons.any_pressed([MouseButton::Right, MouseButton::Middle]) && motion.delta != Vec2::ZERO
    {
        camera
            .0
            .drag(motion.delta.x, motion.delta.y, world.width, world.height);
        ui_state.0.follow_selected = false;
    }
    let steps = match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y,
        MouseScrollUnit::Pixel => scroll.delta.y / PIXELS_PER_SCROLL_STEP,
    };
    if steps != 0.0 {
        let anchor = camera.0.screen_to_world((cursor.x, cursor.y), viewport);
        camera.0.zoom_at(steps, anchor);
        camera.0.clamp_to_world(world.width, world.height);
    }
    if buttons.just_pressed(MouseButton::Left) {
        let (x, y) = camera.0.screen_to_world((cursor.x, cursor.y), viewport);
        let radius = (PICK_RADIUS_PIXELS * camera.0.scale).max(1.0);
        let picked = session.0.simulation().nearest_organism(x, y, radius);
        let state = &mut ui_state.0;
        if picked.is_none() {
            state.follow_selected = false;
        }
        state.selected = picked;
    }
}

/// Hands this frame's real elapsed time to the driver and records frame statistics. The
/// monotonic work timer lives here, in the app; `sim` never reads a clock.
fn step_simulation(
    time: Res<Time<Real>>,
    mut session: ResMut<AppSession>,
    mut stats: ResMut<FrameStats>,
    mut ui_state: ResMut<Ui>,
) {
    let work_started = Instant::now();
    let report = session.0.frame(time.delta(), || work_started.elapsed());
    let sim_time = work_started.elapsed();
    let stats = &mut *stats;
    stats.frames += 1;
    stats
        .window
        .record(time.delta(), sim_time, report.ticks_run);
    stats.last_sim = sim_time;
    stats.last_ticks = report.ticks_run;
    let state = &mut ui_state.0;
    if stats.frames.is_multiple_of(10) {
        state.perf = stats.window.summary();
    }
    state.stall_count = session.0.driver().stall_count();
}

/// Redraws the terrain and food texture: rebuilt after a reset or load (the world size
/// may change), and refreshed every 15 frames while ticks run (SCOPE 7.3).
fn sync_world_texture(
    mut session: ResMut<AppSession>,
    mut textures: ResMut<Textures>,
    mut images: ResMut<Assets<Image>>,
    mut sprite: Single<(&mut Sprite, &mut Transform), With<WorldSprite>>,
    mut pool: ResMut<Pool>,
    stats: Res<FrameStats>,
    mut ui_state: ResMut<Ui>,
    mut dirty: Local<bool>,
) {
    let request = session.0.take_sync();
    let show_food = ui_state.0.show_food;
    match request {
        SyncRequest::Rebuild => {
            let image = world_image(&session.0, show_food);
            textures.world = images.add(image);
            let extent = grid_extent(&session.0);
            let (sprite, transform) = &mut *sprite;
            sprite.image = textures.world.clone();
            sprite.custom_size = Some(extent);
            transform.translation = Vec3::new(extent.x * 0.5, extent.y * 0.5, 0.0);
            pool.shown = None;
            *dirty = false;
            if ui_state
                .0
                .selected
                .is_some_and(|id| session.0.simulation().organism_snapshot(id).is_none())
            {
                ui_state.0.selected = None;
            }
        }
        SyncRequest::Update => *dirty = true,
        SyncRequest::None => {}
    }
    let paused = session.0.driver().is_paused();
    if *dirty && (paused || stats.frames.is_multiple_of(FOOD_REFRESH_FRAMES)) {
        let s = session.0.simulation();
        if let Some(mut image) = images.get_mut(&textures.world)
            && let Some(pixels) = image.data.as_mut()
        {
            render::write_world_pixels(s.terrain(), s.food(), show_food, pixels);
        }
        *dirty = false;
    }
}

/// Positions the sprite pool from the organism slices and moves the selection ring.
fn sync_organisms(
    mut commands: Commands,
    session: Res<AppSession>,
    ui_state: Res<Ui>,
    textures: Res<Textures>,
    mut pool: ResMut<Pool>,
    mut sprites: Query<(&mut Transform, &mut Sprite, &mut Visibility), With<OrganismSprite>>,
    ring: Single<
        (&mut Transform, &mut Sprite, &mut Visibility),
        (With<SelectionMarker>, Without<OrganismSprite>),
    >,
) {
    let s = session.0.simulation();
    let o = s.organisms();
    let state = &ui_state.0;
    let key = (s.tick(), state.color_by, o.len());
    if pool.shown != Some(key) {
        let trait_of = |i: usize| match state.color_by {
            render::ColorBy::Trait(id) => o.trait_values(id)[i],
            render::ColorBy::Species => 0.0,
        };
        let look = |i: usize| {
            let species = o.species()[i];
            let rgb = render::organism_rgb(state.color_by, species, trait_of(i));
            let image = match species {
                Species::Prey => &textures.disc,
                Species::Predator => &textures.triangle,
            };
            let z = if species == Species::Prey { 1.0 } else { 2.0 };
            let rotation = if species == Species::Predator {
                o.headings()[i]
            } else {
                0.0
            };
            let size = render::marker_diameter(o.trait_values(TraitId::Size)[i]);
            (
                image.clone(),
                Color::srgb(rgb[0], rgb[1], rgb[2]),
                z,
                rotation,
                size,
            )
        };
        let existing = pool.entities.len();
        for (k, &entity) in pool.entities.iter().enumerate() {
            let Ok((mut transform, mut sprite, mut visibility)) = sprites.get_mut(entity) else {
                continue;
            };
            if k < o.len() {
                let (image, color, z, rotation, size) = look(k);
                transform.translation = Vec3::new(o.x()[k], o.y()[k], z);
                transform.rotation = Quat::from_rotation_z(rotation);
                if sprite.image.id() != image.id() {
                    sprite.image = image;
                }
                sprite.color = color;
                sprite.custom_size = Some(Vec2::splat(size));
                *visibility = Visibility::Inherited;
            } else {
                *visibility = Visibility::Hidden;
            }
        }
        for k in existing..o.len() {
            let (image, color, z, rotation, size) = look(k);
            let entity = commands
                .spawn((
                    OrganismSprite,
                    Sprite {
                        image,
                        color,
                        custom_size: Some(Vec2::splat(size)),
                        ..default()
                    },
                    Transform::from_xyz(o.x()[k], o.y()[k], z)
                        .with_rotation(Quat::from_rotation_z(rotation)),
                    Visibility::Inherited,
                ))
                .id();
            pool.entities.push(entity);
        }
        pool.shown = Some(key);
    }

    let (mut transform, mut sprite, mut visibility) = ring.into_inner();
    match state.selected.and_then(|id| s.organism_snapshot(id)) {
        Some(snapshot) => {
            transform.translation = Vec3::new(snapshot.x, snapshot.y, 5.0);
            sprite.custom_size = Some(Vec2::splat(
                render::marker_diameter(snapshot.traits.size) + 2.5,
            ));
            *visibility = Visibility::Inherited;
        }
        None => *visibility = Visibility::Hidden,
    }
}

/// Copies the camera view into the Bevy camera; applies fit requests and camera follow.
fn apply_camera(
    session: Res<AppSession>,
    mut ui_state: ResMut<Ui>,
    area: Res<ViewportArea>,
    mut camera: ResMut<WorldCamera>,
    view: Option<Single<(&mut Transform, &mut Projection), With<WorldView>>>,
) {
    let world = &session.0.simulation().config().world;
    let state = &mut ui_state.0;
    if state.fit_requested {
        camera.0 = CameraView::fit(world.width, world.height, area.size.x, area.size.y);
        state.fit_requested = false;
        state.follow_selected = false;
    }
    if state.follow_selected
        && let Some(snapshot) = state
            .selected
            .and_then(|id| session.0.simulation().organism_snapshot(id))
    {
        camera.0.center_x = snapshot.x;
        camera.0.center_y = snapshot.y;
    }
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

/// Builds the panels and windows, then gives the world camera the area between them.
fn ui_system(
    mut contexts: EguiContexts,
    mut session: ResMut<AppSession>,
    mut ui_state: ResMut<Ui>,
    mut area: ResMut<ViewportArea>,
    mut world_camera: Single<&mut Camera, With<WorldView>>,
    window: Single<&Window, With<PrimaryWindow>>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    let session = &mut session.0;
    let state = &mut ui_state.0;
    let mut root = egui::Ui::new(
        ctx.clone(),
        "root".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );
    egui::Panel::top("controls").show(&mut root, |ui| ui::controls::top_bar(ui, session, state));
    egui::Panel::bottom("status").show(&mut root, |ui| ui::controls::status_bar(ui, session));
    egui::Panel::bottom("charts")
        .resizable(true)
        .default_size(270.0)
        .min_size(170.0)
        .show(&mut root, |ui| ui::charts::panel(ui, session, state));
    egui::Panel::left("parameters")
        .resizable(true)
        .default_size(330.0)
        .show(&mut root, |ui| ui::params::panel(ui, session, state));
    egui::Panel::right("inspector")
        .resizable(true)
        .default_size(300.0)
        .show(&mut root, |ui| ui::inspector::panel(ui, session, state));
    let central = root.available_rect_before_wrap();

    if state.show_scenarios {
        ui::scenarios::window(ctx, session, state);
    }
    if state.show_about {
        ui::controls::about(ctx, state);
    }
    if state.show_overlay {
        ui::controls::overlay(ctx, session, state);
    }

    // Give the world camera the free area (physical pixels).
    let scale = window.scale_factor();
    let min = (central.min.to_vec2() * scale).max(egui::Vec2::ZERO);
    let size = central.size() * scale;
    let (width, height) = (window.physical_width(), window.physical_height());
    if size.x >= 1.0 && size.y >= 1.0 && (min.x as u32) < width && (min.y as u32) < height {
        let position = UVec2::new(min.x as u32, min.y as u32);
        let physical = UVec2::new(
            (size.x as u32).min(width - position.x).max(1),
            (size.y as u32).min(height - position.y).max(1),
        );
        world_camera.viewport = Some(Viewport {
            physical_position: position,
            physical_size: physical,
            ..default()
        });
        let new_area = ViewportArea {
            min: Vec2::new(central.min.x, central.min.y),
            size: Vec2::new(central.width(), central.height()),
        };
        let first_layout = area.size == Vec2::new(WINDOW_WIDTH as f32, WINDOW_HEIGHT as f32);
        *area = new_area;
        if first_layout {
            state.fit_requested = true;
        }
    }
    Ok(())
}
