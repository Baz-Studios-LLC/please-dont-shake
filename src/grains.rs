//! Loose grains.
//!
//! The grid alone can't sell a hard shake — cells slumping one step per tick reads as
//! settling, not violence. So grains thrown clear of the grid become real particles
//! with velocity for a second or two, then reintegrate wherever they land. Drama when
//! it's needed, a cheap grid the rest of the time.

use crate::grid::*;
use crate::sand::GrainSpawnQueue;
use crate::tank::TankRoot;
use bevy::prelude::*;

const MAX_GRAINS: usize = 700;
const GRAVITY: f32 = 14.0;
/// Grains are trapped between the glass and the back plate, so they bounce in Z.
const Z_BOUNCE: f32 = 0.35;
const WALL_BOUNCE: f32 = 0.4;
const MAX_LIFE: f32 = 6.0;

#[derive(Component)]
pub struct Grain {
    pub vel: Vec3,
    pub shade: u8,
    pub life: f32,
}

/// One material per palette entry, built once. Grains are opaque little cubes, so
/// they can't share the sand's vertex-coloured material.
#[derive(Resource)]
pub struct GrainAssets {
    pub mesh: Handle<Mesh>,
    pub materials: Vec<Handle<StandardMaterial>>,
}

pub fn setup_grain_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    palette: Res<SandPalette>,
) {
    let mesh = meshes.add(Cuboid::new(CELL * 0.92, CELL * 0.92, CELL * 0.92));
    let materials = palette
        .0
        .iter()
        .map(|c| {
            materials.add(StandardMaterial {
                base_color: *c,
                perceptual_roughness: 0.95,
                reflectance: 0.05,
                ..default()
            })
        })
        .collect();
    commands.insert_resource(GrainAssets { mesh, materials });
}

pub fn spawn_queued_grains(
    mut commands: Commands,
    mut queue: ResMut<GrainSpawnQueue>,
    assets: Res<GrainAssets>,
    tank: Single<Entity, With<TankRoot>>,
    existing: Query<(), With<Grain>>,
) {
    let mut budget = MAX_GRAINS.saturating_sub(existing.iter().count());
    let tank = *tank;

    for spawn in queue.0.drain(..) {
        if budget == 0 {
            break;
        }
        budget -= 1;

        // Spread grains through the slab's depth so they don't all sit on one plane.
        let z = (hash01(spawn.x as u32, spawn.y as u32, 0xD00D) - 0.5) * SLAB_DEPTH * 0.7;
        let mut pos = SandGrid::cell_to_world(spawn.x, spawn.y);
        pos.z = z;

        commands.spawn((
            Grain { vel: spawn.vel, shade: spawn.shade, life: 0.0 },
            Mesh3d(assets.mesh.clone()),
            MeshMaterial3d(
                assets.materials[(spawn.shade as usize).min(assets.materials.len() - 1)].clone(),
            ),
            Transform::from_translation(pos),
            ChildOf(tank),
        ));
    }
}

pub fn update_grains(
    mut commands: Commands,
    time: Res<Time>,
    mut grid: ResMut<SandGrid>,
    mut grains: Query<(Entity, &mut Grain, &mut Transform)>,
) {
    let dt = time.delta_secs();
    let half_w = TANK_W * 0.5;
    let half_h = TANK_H * 0.5;
    let half_d = SLAB_DEPTH * 0.5;

    for (entity, mut grain, mut tf) in &mut grains {
        grain.life += dt;
        grain.vel.y -= GRAVITY * dt;

        let next = tf.translation + grain.vel * dt;

        // Glass front and back plate.
        let mut next = next;
        if next.z.abs() > half_d {
            next.z = next.z.clamp(-half_d, half_d);
            grain.vel.z = -grain.vel.z * Z_BOUNCE;
        }

        // Side walls.
        if next.x.abs() > half_w {
            next.x = next.x.clamp(-half_w, half_w);
            grain.vel.x = -grain.vel.x * WALL_BOUNCE;
        }

        // Above the open top is fine; below the floor is not.
        let (cx, cy) = SandGrid::world_to_cell(next);
        let (cxi, cyi) = (cx.floor() as isize, cy.floor() as isize);

        let hit_floor = next.y < -half_h;
        let hit_sand = SandGrid::in_bounds(cxi, cyi) && !grid.is_air(cxi, cyi);

        // A grain thrown clear of the tank is still in flight, not stuck — gravity
        // will bring it back. Only time out grains that are actually inside the tank,
        // otherwise a high arc gets teleported into the sand mid-flight.
        let above_tank = cyi >= GRID_H as isize;
        let timed_out = !above_tank && grain.life > MAX_LIFE;

        if hit_floor || hit_sand || timed_out {
            let x = cxi.clamp(0, GRID_W as isize - 1);
            let y = if hit_floor { 0 } else { cyi.clamp(0, GRID_H as isize - 1) };
            settle(&mut grid, x, y, grain.shade);
            commands.entity(entity).despawn();
            continue;
        }

        tf.translation = next;
    }
}

/// Put a grain back into the grid, walking up from the impact point to the first free
/// cell. This must never fail: the farm persists for days, and a grain quietly dropped
/// on every shake would slowly empty the tank. So if the whole column is packed, fan
/// outwards until somewhere takes it.
fn settle(grid: &mut SandGrid, cx: isize, cy: isize, shade: u8) {
    // Loose, so it rolls off whatever it landed on and the stream heaps into a cone
    // rather than a spire. It packs the moment it runs out of downhill.
    let place = |grid: &mut SandGrid, x: isize, y: isize| {
        grid.set_loose(x as usize, y as usize, Cell { mat: Substance::Sand, shade });
    };

    for y in cy.max(0)..GRID_H as isize {
        if grid.is_air(cx, y) {
            place(grid, cx, y);
            return;
        }
    }

    for dx in 1..GRID_W as isize {
        for nx in [cx - dx, cx + dx] {
            if nx < 0 || nx >= GRID_W as isize {
                continue;
            }
            for y in cy.max(0)..GRID_H as isize {
                if grid.is_air(nx, y) {
                    place(grid, nx, y);
                    return;
                }
            }
        }
    }
}
