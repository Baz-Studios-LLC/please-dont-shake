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
            // If there is genuinely nowhere for it, it stays a particle and tries again next
            // frame rather than being despawned into nothing. Gravity is still acting on it and
            // the grid is still moving, so "nowhere" is a momentary state, not a permanent one.
            if settle(&mut grid, x, y, grain.shade) {
                commands.entity(entity).despawn();
            }
            continue;
        }

        tf.translation = next;
    }
}

/// Put a grain back into the grid, walking up from the impact point to the first free
/// cell. This must never fail: the farm persists for days, and a grain quietly dropped
/// on every shake would slowly empty the tank. So if the whole column is packed, fan
/// outwards until somewhere takes it.
pub fn settle(grid: &mut SandGrid, cx: isize, cy: isize, shade: u8) -> bool {
    // Loose, so it rolls off whatever it landed on and the stream heaps into a cone
    // rather than a spire. It packs the moment it runs out of downhill.
    let place = |grid: &mut SandGrid, x: isize, y: isize| {
        grid.set_loose(x as usize, y as usize, Cell { mat: Substance::Sand, shade });
    };

    for y in cy.max(0)..GRID_H as isize {
        if grid.is_air(cx, y) {
            place(grid, cx, y);
            return true;
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
                    return true;
                }
            }
        }
    }

    // Nowhere at all. Every caller consumes the grain it is handing over — a particle is
    // despawned, an ant is despawned, a queen has already taken the cell out of the ground — so
    // returning quietly here destroys sand, which is the one thing this game promises never to
    // do. It says so instead, and each caller decides: keep the particle in the air, put the
    // grain back where it came from, or at minimum say so out loud.
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sand() -> Cell {
        Cell { mat: Substance::Sand, shade: 3 }
    }

    /// The contract every caller depends on: a grain handed to `settle` either enters the grid
    /// exactly once, or is refused so the caller can keep it.
    ///
    /// This is the test that was missing when the queen's founding spoil was handed
    /// `GRID_H - 2` and quietly vanished whenever the top of the tank was full. `settle`
    /// searches *upward*, so "somewhere above here" and "somewhere in the tank" are different
    /// questions, and only one of them is safe to answer with silence.
    #[test]
    fn a_grain_lands_exactly_once_or_is_refused() {
        let mut grid = SandGrid::new();

        // Open tank: it lands, and the count goes up by one.
        let before = grid.sand_count();
        assert!(settle(&mut grid, 40, 0, 3), "an empty tank refused a grain");
        assert_eq!(grid.sand_count(), before + 1, "a grain did not land exactly once");

        // A full column still lands — in a neighbour, which is what the fallback is for.
        for y in 0..GRID_H {
            grid.set_raw(80, y, sand());
        }
        let before = grid.sand_count();
        assert!(settle(&mut grid, 80, 0, 3), "a full column refused a grain");
        assert_eq!(grid.sand_count(), before + 1);

        // Asked to land above a full column, with the rest of the tank open, it still finds air.
        let before = grid.sand_count();
        assert!(settle(&mut grid, 80, GRID_H as isize - 2, 3), "refused near the roof");
        assert_eq!(grid.sand_count(), before + 1);
    }

    /// Nowhere at all has to be *reported*, not swallowed. Every caller consumes the grain it
    /// hands over — a despawned particle, a dead ant, a cell the queen has already taken out of
    /// the ground — so a silent failure here is sand ceasing to exist.
    #[test]
    fn a_tank_with_no_air_refuses_the_grain() {
        let mut grid = SandGrid::new();
        for x in 0..GRID_W {
            for y in 0..GRID_H {
                grid.set_raw(x, y, sand());
            }
        }
        let before = grid.sand_count();
        assert!(!settle(&mut grid, 40, 0, 3), "a solid tank claimed to have taken a grain");
        assert_eq!(grid.sand_count(), before, "a refused grain still changed the grid");
    }

    /// It may never overwrite. Landing on top of what is there is the whole reason it walks up.
    #[test]
    fn settling_never_overwrites_what_is_already_there() {
        let mut grid = SandGrid::new();
        grid.set_raw(50, 0, Cell { mat: Substance::Sand, shade: 7 });
        assert!(settle(&mut grid, 50, 0, 3));
        assert_eq!(grid.get(50, 0).shade, 7, "settle overwrote an occupied cell");
        assert_eq!(grid.get(50, 1).shade, 3, "the new grain did not land above it");
    }
}
