//! `genesis-app` — the windowed Bevy shell (Phase 6 step 2, plus the GPU
//! half of step 3 and the input half of step 4).
//!
//! Architecture per docs/research/render-bootstrap.md (Q-2026-07-09-B):
//! lockstep with an extraction seam. This app owns the `Simulation`, ticks
//! it inside the frame loop (0 when paused, whole ticks under warp via
//! `WarpPacer` — the wall clock never enters the sim), then extracts a
//! `RenderFrame` of plain data that rendering alone consumes. All
//! simulation-facing logic lives in the headless-tested library modules;
//! this file only wires window, input, and GPU resources around them.
//!
//! Player input is emitted as ordinary replay-recorded `PlayerAction`s
//! through `Simulation::queue_action` — the UI is just another script
//! author (Q-2026-07-08-B).

use std::path::PathBuf;

use bevy::prelude::*;
use clap::Parser;

use genesis_config::{ActionScript, RulePack, SimConfig};
use genesis_render::brush::{BrushStamp, FieldOp, stamp_actions};
use genesis_render::pacer::WarpPacer;
use genesis_render::raster::{HeatmapMapping, PaletteRamp, WorldRect, rasterize};
use genesis_render::{Camera as ViewCamera, RenderFrame, Tier, VisualMapping, extract};
use genesis_sim::Simulation;
use genesis_sim::interact::RuleSet;

/// Windowed Genesis: watch and shape a running simulation.
#[derive(Parser, Debug)]
#[command(name = "genesis-app")]
struct Args {
    /// RON simulation config (defaults to the built-in config).
    #[arg(long)]
    config: Option<PathBuf>,
    /// RON interaction rule pack.
    #[arg(long)]
    rules: Option<PathBuf>,
    /// RON player-action script to replay on top of live play.
    #[arg(long)]
    actions: Option<PathBuf>,
    /// RON quantity→visual mapping (hot-swappable data, never replay identity).
    #[arg(long)]
    mapping: Option<PathBuf>,
    /// RON palette ramp for the heatmap tiers (see palettes/).
    #[arg(long)]
    palette: Option<PathBuf>,
    /// Exit automatically after this many frames (headless-ish smoke test);
    /// prints the final tick and state hash to stdout.
    #[arg(long)]
    smoke: Option<u32>,
    /// Initial view width in world units (default: the whole world).
    #[arg(long)]
    zoom: Option<f32>,
}

/// The owned simulation — the only mutable simulation state in the app.
#[derive(Resource)]
struct Sim(Simulation);

/// Camera over the torus world (world units; +y down, matching the raster
/// and inspector convention). Bevy's screen-space camera stays fixed.
#[derive(Resource)]
struct View {
    cam: ViewCamera,
    world_w: f32,
    world_h: f32,
}

/// Warp pacing + the honesty display's achieved-rate window.
#[derive(Resource)]
struct Pacing {
    pacer: WarpPacer,
    /// Rate for the 1× preset (1/dt ticks per wall second).
    base_rate: f64,
    /// Rate to restore when unpausing.
    resume_rate: f64,
    starved: bool,
    window_ticks: u32,
    window_secs: f64,
    achieved: f64,
}

/// Visual mapping data (swappable, never replay identity).
#[derive(Resource)]
struct Mappings {
    sprite: VisualMapping,
    heat: HeatmapMapping,
}

#[derive(Resource)]
struct Smoke {
    frames_left: Option<u32>,
}

/// Declared env-field count, fixed at construction — the brush needs a
/// target field and must not pay a full snapshot per drag frame to ask.
#[derive(Resource)]
struct EnvFieldCount(usize);

/// GPU-side handles: the shared soft-dot texture, the heatmap image and the
/// entity that displays it, and the sprite entity pool.
#[derive(Resource)]
struct DrawAssets {
    dot: Handle<Image>,
    heat_image: Handle<Image>,
    heat_size: UVec2,
    heat_entity: Entity,
    pool: Vec<Entity>,
}

#[derive(Component)]
struct DotSprite;

#[derive(Component)]
struct HeatSprite;

#[derive(Component)]
struct Hud;

/// Hard cap on live sprite entities: past this the view is a field, not
/// individuals, and the tier thresholds normally switch long before here.
const MAX_SPRITES: usize = 60_000;
/// Integer downscale factor for the heatmap offscreen target (step 3: low-res
/// raster + nearest-neighbor upscale = the retro pixel look).
const HEAT_SCALE: u32 = 4;
/// Per-frame whole-tick allowance handed to the pacer.
const TICK_BUDGET: u32 = 240;

fn main() {
    let args = Args::parse();

    let config = match &args.config {
        Some(p) => SimConfig::load(p).unwrap_or_else(|e| panic!("{}: {e}", p.display())),
        None => SimConfig::default(),
    };
    let rules = match &args.rules {
        Some(p) => {
            RuleSet::compile(&RulePack::load(p).unwrap_or_else(|e| panic!("{}: {e}", p.display())))
        }
        None => RuleSet::default(),
    };
    let script = match &args.actions {
        Some(p) => ActionScript::load(p).unwrap_or_else(|e| panic!("{}: {e}", p.display())),
        None => ActionScript::default(),
    };
    let sprite_mapping = match &args.mapping {
        Some(p) => VisualMapping::load(p).unwrap_or_else(|e| panic!("{}: {e}", p.display())),
        None => VisualMapping::default(),
    };
    let mut heat = HeatmapMapping::default();
    if let Some(p) = &args.palette {
        heat.palette = PaletteRamp::load(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    }

    let sim = Simulation::with_rules_and_actions(&config, rules, script);
    let base_rate = 1.0 / config.dt() as f64;

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Project Genesis".into(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(ClearColor(Color::srgb(0.02, 0.02, 0.05)))
        .insert_resource(Sim(sim))
        .insert_resource(View {
            cam: ViewCamera {
                center_x: config.world_width * 0.5,
                center_y: config.world_height * 0.5,
                width: args
                    .zoom
                    .unwrap_or(config.world_width)
                    .clamp(config.world_width * 0.005, config.world_width),
            },
            world_w: config.world_width,
            world_h: config.world_height,
        })
        .insert_resource(Pacing {
            pacer: WarpPacer::new(base_rate),
            base_rate,
            resume_rate: base_rate,
            starved: false,
            window_ticks: 0,
            window_secs: 0.0,
            achieved: 0.0,
        })
        .insert_resource(Mappings {
            sprite: sprite_mapping,
            heat,
        })
        .insert_resource(Smoke {
            frames_left: args.smoke,
        })
        .insert_resource(EnvFieldCount(config.env.fields.len()))
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                input_camera,
                input_time,
                input_brush,
                run_ticks,
                draw_frame,
                smoke_exit,
            )
                .chain(),
        )
        .run();
}

/// Soft radial dot: white core, alpha falling to zero at the rim. One texture
/// shared by every particle sprite; color/tint carries the mapping.
fn make_dot_image() -> Image {
    use bevy::asset::RenderAssetUsages;
    use bevy::image::ImageSampler;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

    const N: u32 = 32;
    let mut data = Vec::with_capacity((N * N * 4) as usize);
    let c = (N as f32 - 1.0) * 0.5;
    for y in 0..N {
        for x in 0..N {
            let dx = (x as f32 - c) / c;
            let dy = (y as f32 - c) / c;
            let d = (dx * dx + dy * dy).sqrt();
            let a = ((1.0 - d) * 1.6).clamp(0.0, 1.0);
            let a = (a * a * 255.0) as u8;
            data.extend_from_slice(&[255, 255, 255, a]);
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: N,
            height: N,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.sampler = ImageSampler::linear();
    image
}

fn make_heat_image(size: UVec2) -> Image {
    use bevy::asset::RenderAssetUsages;
    use bevy::image::ImageSampler;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

    let mut image = Image::new_fill(
        Extent3d {
            width: size.x.max(1),
            height: size.y.max(1),
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 255],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    // Nearest-neighbor: the integer upscale must show crisp raster pixels
    // (visuals.md), not a blur.
    image.sampler = ImageSampler::nearest();
    image
}

fn setup(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    commands.spawn(Camera2d);

    let dot = images.add(make_dot_image());
    let heat_size = UVec2::new(320, 180);
    let heat_image = images.add(make_heat_image(heat_size));
    let heat_entity = commands
        .spawn((
            HeatSprite,
            Sprite {
                image: heat_image.clone(),
                ..default()
            },
            Transform::from_xyz(0.0, 0.0, -1.0),
            Visibility::Hidden,
        ))
        .id();

    commands.insert_resource(DrawAssets {
        dot,
        heat_image,
        heat_size,
        heat_entity,
        pool: Vec::new(),
    });

    commands.spawn((
        Hud,
        Text::new(""),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(8.0),
            top: Val::Px(6.0),
            ..default()
        },
    ));
}

/// Pan (WASD/arrows or right/middle drag) and zoom (wheel, cursor-centered
/// enough for v1: zooms about the view center). The camera is presentation
/// only — it never touches the simulation.
fn input_camera(
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut wheel: MessageReader<bevy::input::mouse::MouseWheel>,
    mut motion: MessageReader<bevy::input::mouse::MouseMotion>,
    windows: Query<&Window>,
    time: Res<Time>,
    mut view: ResMut<View>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let win_w = window.width().max(1.0);

    // Zoom: wheel steps scale the view width, clamped to sane bounds.
    let mut zoom = 0.0f32;
    for ev in wheel.read() {
        zoom += ev.y;
    }
    if zoom != 0.0 {
        let factor = 0.9f32.powf(zoom);
        view.cam.width = (view.cam.width * factor).clamp(view.world_w * 0.005, view.world_w);
    }

    // World units per screen pixel at the current zoom.
    let upp = view.cam.width / win_w;

    let mut dx = 0.0f32;
    let mut dy = 0.0f32;
    let pan = view.cam.width * 0.75 * time.delta_secs();
    if keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft) {
        dx -= pan;
    }
    if keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight) {
        dx += pan;
    }
    if keys.pressed(KeyCode::KeyW) || keys.pressed(KeyCode::ArrowUp) {
        dy -= pan;
    }
    if keys.pressed(KeyCode::KeyS) || keys.pressed(KeyCode::ArrowDown) {
        dy += pan;
    }
    let dragging = buttons.pressed(MouseButton::Right) || buttons.pressed(MouseButton::Middle);
    for ev in motion.read() {
        if dragging {
            dx -= ev.delta.x * upp;
            dy -= ev.delta.y * upp;
        }
    }
    if dx != 0.0 || dy != 0.0 {
        view.cam.center_x = genesis_core::torus::wrap(view.cam.center_x + dx, view.world_w);
        view.cam.center_y = genesis_core::torus::wrap(view.cam.center_y + dy, view.world_h);
    }
}

/// Time controls: space pauses/resumes, digits pick warp presets. Warp is
/// more ticks per wall second, never a bigger dt.
fn input_time(keys: Res<ButtonInput<KeyCode>>, mut pacing: ResMut<Pacing>) {
    let presets = [
        (KeyCode::Digit1, 1.0),
        (KeyCode::Digit2, 4.0),
        (KeyCode::Digit3, 16.0),
        (KeyCode::Digit4, 64.0),
    ];
    for (key, mult) in presets {
        if keys.just_pressed(key) {
            let rate = pacing.base_rate * mult;
            pacing.pacer.set_rate(rate);
            pacing.resume_rate = rate;
        }
    }
    if keys.just_pressed(KeyCode::Space) {
        if pacing.pacer.target_rate() > 0.0 {
            pacing.pacer.set_rate(0.0);
        } else {
            let rate = pacing.resume_rate;
            pacing.pacer.set_rate(rate);
        }
    }
}

/// Field brush (step 4): left-drag adds to env field 0 under the cursor.
/// The stamp becomes ordinary replay-recorded `PlayerAction`s through the
/// one scripted-action path; a stamp near the seam wraps into 2 or 4 rects.
/// Does nothing when the config declares no env fields.
fn input_brush(
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    view: Res<View>,
    fields: Res<EnvFieldCount>,
    mut sim: ResMut<Sim>,
) {
    if !buttons.pressed(MouseButton::Left) || fields.0 == 0 {
        return;
    }
    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let (wx, wy) = view.cam.world_from_screen(
        cursor.x,
        cursor.y,
        window.width().max(1.0),
        window.height().max(1.0),
        view.world_w,
        view.world_h,
    );
    let radius = view.cam.width * 0.03;
    let now = sim.0.tick_count();
    for action in stamp_actions(
        BrushStamp {
            x: wx,
            y: wy,
            half_w: radius,
            half_h: radius,
        },
        0,
        FieldOp::Add(0.05),
        now,
        view.world_w,
        view.world_h,
    ) {
        // Live input: report-and-drop, never fatal.
        if let Err(e) = sim.0.queue_action(action) {
            warn!("brush action rejected: {e}");
        }
    }
}

/// The lockstep core: plan whole ticks from measured frame time, run exactly
/// that many. The wall clock decides how many ticks — never what they do.
fn run_ticks(time: Res<Time>, mut pacing: ResMut<Pacing>, mut sim: ResMut<Sim>) {
    let dt = time.delta_secs_f64();
    let plan = pacing.pacer.ticks_for(dt, TICK_BUDGET);
    for _ in 0..plan.ticks {
        sim.0.tick();
    }
    pacing.starved = plan.starved;
    // Rolling achieved-rate window for the honesty display.
    pacing.window_ticks += plan.ticks;
    pacing.window_secs += dt;
    if pacing.window_secs >= 0.5 {
        pacing.achieved = pacing.window_ticks as f64 / pacing.window_secs;
        pacing.window_ticks = 0;
        pacing.window_secs = 0.0;
    }
}

/// The mutable sprite parts both draw paths update. `Without` keeps the two
/// queries provably disjoint so Bevy allows them in one system.
type SpriteParts = (
    &'static mut Sprite,
    &'static mut Transform,
    &'static mut Visibility,
);
type DotQuery<'w, 's> = Query<'w, 's, SpriteParts, (With<DotSprite>, Without<HeatSprite>)>;
type HeatQuery<'w, 's> = Query<'w, 's, SpriteParts, (With<HeatSprite>, Without<DotSprite>)>;

/// Extract the frame and update GPU-side entities: sprite pool + gizmo bonds
/// at T0/T1, rasterized heatmap texture at T2/T3, HUD text always.
#[allow(clippy::too_many_arguments)]
fn draw_frame(
    mut commands: Commands,
    windows: Query<&Window>,
    sim: Res<Sim>,
    view: Res<View>,
    pacing: Res<Pacing>,
    mappings: Res<Mappings>,
    mut assets: ResMut<DrawAssets>,
    mut images: ResMut<Assets<Image>>,
    mut sprites: DotQuery,
    mut heat_sprite: HeatQuery,
    mut hud: Query<&mut Text, With<Hud>>,
    mut gizmos: Gizmos,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let (win_w, win_h) = (window.width().max(1.0), window.height().max(1.0));
    let snap = sim.0.snapshot();
    let frame = extract(
        &snap,
        &view.cam,
        win_w as u32,
        win_h as u32,
        &mappings.sprite,
    );
    // Pixels per world unit; extraction gives camera-relative world units,
    // +y down. Bevy's 2D camera is +y up, so y flips at the transform.
    let scale = win_w / view.cam.width;

    match frame.tier {
        Tier::T0Particle | Tier::T1Cluster => {
            draw_sprites(&mut commands, &frame, scale, &mut assets, &mut sprites);
            for b in &frame.bonds {
                gizmos.line_2d(
                    Vec2::new(b.x0 * scale, -b.y0 * scale),
                    Vec2::new(b.x1 * scale, -b.y1 * scale),
                    Color::srgba(0.7, 0.8, 1.0, b.alpha),
                );
            }
            if let Ok((_, _, mut vis)) = heat_sprite.get_mut(assets.heat_entity) {
                *vis = Visibility::Hidden;
            }
        }
        Tier::T2Field | Tier::T3Planet => {
            // Hide every pooled sprite.
            for &e in &assets.pool {
                if let Ok((_, _, mut vis)) = sprites.get_mut(e) {
                    *vis = Visibility::Hidden;
                }
            }
            let grid = frame.cells.as_ref().expect("heatmap tier carries cells");
            let target = UVec2::new(
                (win_w as u32 / HEAT_SCALE).max(1),
                (win_h as u32 / HEAT_SCALE).max(1),
            );
            if target != assets.heat_size {
                assets.heat_size = target;
                assets.heat_image = images.add(make_heat_image(target));
            }
            let view_h = view.cam.width * win_h / win_w;
            let rect = WorldRect {
                x: view.cam.center_x - 0.5 * view.cam.width,
                y: view.cam.center_y - 0.5 * view_h,
                w: view.cam.width,
                h: view_h,
            };
            let bytes = rasterize(grid, &rect, target.x, target.y, &mappings.heat);
            if let Some(mut image) = images.get_mut(&assets.heat_image) {
                image.data = Some(bytes);
            }
            if let Ok((mut sprite, mut tf, mut vis)) = heat_sprite.get_mut(assets.heat_entity) {
                sprite.image = assets.heat_image.clone();
                sprite.custom_size = Some(Vec2::new(win_w, win_h));
                *tf = Transform::from_xyz(0.0, 0.0, -1.0);
                *vis = Visibility::Visible;
            }
        }
    }

    if let Ok(mut text) = hud.single_mut() {
        let target = pacing.pacer.target_rate();
        let status = if target <= 0.0 {
            "paused".to_string()
        } else {
            format!(
                "{:.0}/{:.0} ticks/s{}",
                pacing.achieved,
                target,
                if pacing.starved { " (starved)" } else { "" }
            )
        };
        text.0 = format!(
            "tick {}  |  {}  |  {:?}  |  {} particles  |  spin {:.2}\n\
             space pause  1-4 warp  wheel zoom  WASD/right-drag pan  left-drag brush",
            snap.tick,
            status,
            frame.tier,
            snap.particles.len(),
            snap.spin,
        );
    }
}

fn draw_sprites(
    commands: &mut Commands,
    frame: &RenderFrame,
    scale: f32,
    assets: &mut DrawAssets,
    sprites: &mut DotQuery,
) {
    let wanted = frame.sprites.len().min(MAX_SPRITES);
    // Grow the pool to demand; entities are reused forever after.
    while assets.pool.len() < wanted {
        let e = commands
            .spawn((
                DotSprite,
                Sprite {
                    image: assets.dot.clone(),
                    ..default()
                },
                Transform::default(),
                Visibility::Hidden,
            ))
            .id();
        assets.pool.push(e);
    }
    for (i, &entity) in assets.pool.iter().enumerate() {
        let Ok((mut sprite, mut tf, mut vis)) = sprites.get_mut(entity) else {
            continue;
        };
        if i >= wanted {
            *vis = Visibility::Hidden;
            continue;
        }
        let inst = &frame.sprites[i];
        // Radius in world units → sprite diameter in pixels (floor of 2px so
        // zoomed-out particles stay visible).
        let d = (inst.radius * 2.0 * scale).max(2.0);
        sprite.custom_size = Some(Vec2::splat(d));
        sprite.color = Color::hsla(inst.hue * 360.0, 0.75, 0.35 + 0.45 * inst.brightness, 1.0);
        *tf = Transform::from_xyz(inst.x * scale, -inst.y * scale, 0.0);
        *vis = Visibility::Visible;
    }
}

/// Smoke mode: run N frames, print the outcome, exit. Lets a terminal
/// session verify the windowed loop end-to-end without a human.
fn smoke_exit(mut smoke: ResMut<Smoke>, sim: Res<Sim>, mut exit: MessageWriter<AppExit>) {
    let Some(left) = smoke.frames_left.as_mut() else {
        return;
    };
    if *left > 0 {
        *left -= 1;
        return;
    }
    println!(
        "smoke ok: tick {} hash {:#018x}",
        sim.0.tick_count(),
        sim.0.state_hash()
    );
    exit.write(AppExit::Success);
}
