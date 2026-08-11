//! The tank itself: frame, glass, back plate, camera and lighting — plus the spring
//! that lets the whole thing lurch when you grab it.
//!
//! The camera never moves. Shaking moves the *tank*, inside a stable frame. Camera
//! shake would be nauseating and would hide the chaos we're paying for.

use crate::grid::*;
use crate::meshing::{SandChunk, SandMaterial, build_chunk_mesh};
use crate::sand::TankMotion;
use bevy::prelude::*;

/// Everything inside the glass hangs off this, so one transform shakes the lot.
#[derive(Component)]
pub struct TankRoot;

/// The camera that looks at the tank.
///
/// Named because the hand adds a second one, and `Single<&Camera>` silently *skips* its
/// system when two match — which is not an error, just a game where nothing responds to the
/// mouse any more. Anything wanting the view the player is looking through asks for this.
#[derive(Component)]
pub struct TankCamera;

#[derive(Resource, Default)]
pub struct TankSpring {
    pub offset: Vec3,
    pub vel: Vec3,
    /// Radians. A shake should rock the tank a little, not just slide it.
    pub tilt: f32,
    pub tilt_vel: f32,
}

/// The backdrop room, sized to overfill the camera frustum at its depth so it covers the
/// frame on any window aspect. Keeps the source image's 16:9 proportions.
const ROOM_Z: f32 = -6.0;
const ROOM_H: f32 = 15.0;
const ROOM_W: f32 = ROOM_H * 16.0 / 9.0;

/// How far back the camera sits. Shared with `interact`, which needs it to convert
/// pointer pixels into world units at the tank's depth.
pub const CAM_DIST: f32 = 10.4;

const SPRING_STIFFNESS: f32 = 240.0;
const SPRING_DAMPING: f32 = 11.0;
const TILT_STIFFNESS: f32 = 190.0;
const TILT_DAMPING: f32 = 9.5;
/// How far the tank can be pulled from rest, in world units.
const MAX_OFFSET: f32 = 1.1;
const MAX_TILT: f32 = 0.075;

pub fn setup_tank(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut grid: ResMut<SandGrid>,
    palette: Res<SandPalette>,
) {
    // One material for all sand. Colour rides on the vertices, so base must be white.
    let sand_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 0.96,
        reflectance: 0.06,
        ..default()
    });
    commands.insert_resource(SandMaterial(sand_mat.clone()));

    // The back pane is a dark, translucent sheet rather than a solid wall, so the room
    // behind the tank glows faintly through every tunnel. That's what gives near-black
    // ants something to read against — the first build had them invisible in the dark —
    // while still keeping tunnels darker than the lit sand around them.
    let back_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.16, 0.135, 0.125, 0.72),
        perceptual_roughness: 0.98,
        reflectance: 0.02,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });

    // Barely-there glass. Almost all of the read comes from the specular highlight,
    // not from tinting what's behind it.
    let glass_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.80, 0.90, 0.92, 0.055),
        perceptual_roughness: 0.035,
        metallic: 0.0,
        reflectance: 0.42,
        alpha_mode: AlphaMode::Blend,
        double_sided: true,
        cull_mode: None,
        ..default()
    });

    let wood_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.28, 0.185, 0.12),
        perceptual_roughness: 0.72,
        reflectance: 0.10,
        ..default()
    });

    let zf = SLAB_DEPTH * 0.5;
    let zb = -SLAB_DEPTH * 0.5;

    let frame_t = 0.34;
    let frame_d = SLAB_DEPTH + 0.34;
    let frame_mesh_v = meshes.add(Cuboid::new(frame_t, TANK_H + frame_t * 2.0, frame_d));
    let frame_mesh_h = meshes.add(Cuboid::new(TANK_W + frame_t * 2.0, frame_t, frame_d));

    let back_mesh = meshes.add(Rectangle::new(TANK_W, TANK_H));
    let glass_mesh = meshes.add(Rectangle::new(TANK_W, TANK_H));

    commands
        .spawn((
            TankRoot,
            Transform::default(),
            Visibility::default(),
            Name::new("Tank"),
        ))
        .with_children(|tank| {
            // Back plate. Dark, so tunnels read as voids rather than holes onto nothing.
            tank.spawn((
                Mesh3d(back_mesh),
                MeshMaterial3d(back_mat),
                Transform::from_xyz(0.0, 0.0, zb - 0.015),
            ));

            // Sand chunks.
            for cy in 0..CHUNKS_Y {
                for cx in 0..CHUNKS_X {
                    let mesh = build_chunk_mesh(&grid, &palette, cx, cy);
                    let mut chunk = tank.spawn((
                        SandChunk { cx, cy },
                        MeshMaterial3d(sand_mat.clone()),
                        Transform::default(),
                        Visibility::Inherited,
                    ));
                    // A third of the tank is open air above the sand. Those chunks get
                    // no mesh at all until sand reaches them — uploading empty geometry
                    // buys nothing and upsets the renderer's mesh allocator.
                    if mesh.count_vertices() > 0 {
                        chunk.insert(Mesh3d(meshes.add(mesh)));
                    }
                }
            }
            // Every chunk mesh is now current, so don't let frame one rebuild them all
            // over again — the redundant upload is both wasted work and enough to upset
            // the renderer's mesh allocator.
            grid.dirty.fill(false);

            // Glass, sitting just proud of the sand.
            tank.spawn((
                Mesh3d(glass_mesh),
                MeshMaterial3d(glass_mat),
                Transform::from_xyz(0.0, 0.0, zf + 0.09),
            ));

            // Frame.
            let hx = TANK_W * 0.5 + frame_t * 0.5;
            let hy = TANK_H * 0.5 + frame_t * 0.5;
            for (mesh, x, y) in [
                (frame_mesh_v.clone(), -hx, 0.0),
                (frame_mesh_v.clone(), hx, 0.0),
                (frame_mesh_h.clone(), 0.0, -hy),
                (frame_mesh_h.clone(), 0.0, hy),
            ] {
                tank.spawn((
                    Mesh3d(mesh),
                    MeshMaterial3d(wood_mat.clone()),
                    Transform::from_xyz(x, y, 0.0),
                ));
            }
        });

    // The room the farm is sitting in, seen through both panes of glass.
    //
    // Deliberately *not* a child of the tank: the room doesn't move when you grab the
    // farm, and that contrast is most of what sells the shake. It's also why the camera
    // never moves — with a fixed room behind it, a lurching tank reads as the tank
    // lurching rather than as the view being jostled.
    //
    // Unlit, because the image already carries its own golden-hour lighting and our tank
    // lights have no business touching it.
    let room_mat = materials.add(StandardMaterial {
        base_color_texture: Some(asset_server.load("kids-bedroom-sunny-through-glass.png")),
        unlit: true,
        ..default()
    });
    commands.spawn((
        Mesh3d(meshes.add(Rectangle::new(ROOM_W, ROOM_H))),
        MeshMaterial3d(room_mat),
        Transform::from_xyz(0.0, 0.0, ROOM_Z),
        Name::new("Room"),
    ));

    // Camera. Framed so the tank fills most of the view; it never moves again.
    //
    // `IsDefaultUiCamera` is load-bearing now that the hand adds a second camera: with two
    // in the world, which one Bevy attaches the interface to stops being obvious, and the
    // menu attaching to the hand's overlay would put it on a layer nothing else draws.
    // Stating it costs nothing and this project has already lost the title screen twice to
    // camera plumbing.
    commands.spawn((
        TankCamera,
        Camera3d::default(),
        bevy::ui::IsDefaultUiCamera,
        Transform::from_xyz(0.0, 0.0, CAM_DIST).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Key light from the front-upper-left. The grazing angle is deliberate: it lets
    // tunnels cast shadows into themselves, which is most of what sells the depth.
    commands.spawn((
        DirectionalLight {
            illuminance: 5200.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(-5.0, 7.0, 9.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // A dim fill from the opposite side so the shadowed walls don't go to pure black.
    commands.spawn((
        DirectionalLight {
            illuminance: 1100.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(6.0, -3.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    commands.insert_resource(GlobalAmbientLight {
        color: Color::srgb(0.80, 0.86, 1.0),
        brightness: 260.0,
        ..default()
    });
}

/// Integrate the tank's spring and publish its velocity for the sand sim to read.
pub fn tank_spring(
    time: Res<Time>,
    mut spring: ResMut<TankSpring>,
    mut motion: ResMut<TankMotion>,
    mut tank: Single<&mut Transform, With<TankRoot>>,
) {
    let dt = time.delta_secs().min(1.0 / 30.0);

    let accel = -spring.offset * SPRING_STIFFNESS - spring.vel * SPRING_DAMPING;
    spring.vel += accel * dt;
    let v = spring.vel;
    spring.offset = (spring.offset + v * dt).clamp_length_max(MAX_OFFSET);

    let t_accel = -spring.tilt * TILT_STIFFNESS - spring.tilt_vel * TILT_DAMPING;
    spring.tilt_vel += t_accel * dt;
    let tv = spring.tilt_vel;
    spring.tilt = (spring.tilt + tv * dt).clamp(-MAX_TILT, MAX_TILT);

    tank.translation = spring.offset;
    tank.rotation = Quat::from_rotation_z(spring.tilt);

    motion.vel = spring.vel;
}
