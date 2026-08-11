//! Turning the grid into geometry.
//!
//! The sand is a slab with real thickness, not a flat picture of sand. Every solid
//! cell contributes a front face, and any face exposed to air also gets built — so
//! when the ants dig, the tunnel has walls that recede back toward the tank's rear
//! plate and catch the light properly.
//!
//! Meshes are per chunk and only rebuilt when that chunk is marked dirty, which keeps
//! a settled farm at effectively zero cost.

use crate::grid::*;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

/// Flat per-direction shading. Cheap, and it's what gives the chunky-voxel read even
/// before any real lighting lands on it.
const TINT_FRONT: f32 = 1.0;
const TINT_TOP: f32 = 1.15;
const TINT_BOTTOM: f32 = 0.52;
const TINT_SIDE: f32 = 0.74;

#[derive(Component)]
pub struct SandChunk {
    pub cx: usize,
    pub cy: usize,
}

/// Handle to the one material every sand chunk shares. Colour comes from vertex
/// attributes, so a single white material covers all forty palette entries.
/// Held for M2, when the ants start spawning and despawning chunks of their own.
#[derive(Resource)]
#[allow(dead_code)]
pub struct SandMaterial(pub Handle<StandardMaterial>);

struct MeshBuf {
    pos: Vec<[f32; 3]>,
    nrm: Vec<[f32; 3]>,
    col: Vec<[f32; 4]>,
    uv: Vec<[f32; 2]>,
    idx: Vec<u32>,
}

impl MeshBuf {
    fn new() -> Self {
        Self { pos: vec![], nrm: vec![], col: vec![], uv: vec![], idx: vec![] }
    }

    /// Corners must be given counter-clockwise as seen from `normal`.
    fn quad(&mut self, corners: [Vec3; 4], normal: Vec3, colour: [f32; 4], tint: f32) {
        let base = self.pos.len() as u32;
        let c = [colour[0] * tint, colour[1] * tint, colour[2] * tint, colour[3]];
        const UVS: [[f32; 2]; 4] = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
        for (i, v) in corners.iter().enumerate() {
            self.pos.push([v.x, v.y, v.z]);
            self.nrm.push([normal.x, normal.y, normal.z]);
            self.col.push(c);
            self.uv.push(UVS[i]);
        }
        self.idx
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    fn into_mesh(self) -> Mesh {
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.nrm)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.col)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uv)
        .with_inserted_indices(Indices::U32(self.idx))
    }
}

pub fn build_chunk_mesh(grid: &SandGrid, palette: &SandPalette, cx: usize, cy: usize) -> Mesh {
    let mut buf = MeshBuf::new();

    let zf = SLAB_DEPTH * 0.5;
    let zb = -SLAB_DEPTH * 0.5;

    for y in cy * CHUNK..(cy + 1) * CHUNK {
        for x in cx * CHUNK..(cx + 1) * CHUNK {
            let cell = grid.get(x, y);
            if cell.mat == Substance::Air {
                continue;
            }

            let centre = SandGrid::cell_to_world(x, y);
            let x0 = centre.x - CELL * 0.5;
            let x1 = centre.x + CELL * 0.5;
            let y0 = centre.y - CELL * 0.5;
            let y1 = centre.y + CELL * 0.5;

            let colour = palette.linear(cell.shade);
            let (xi, yi) = (x as isize, y as isize);

            // Front — always visible, nothing is ever in front of the sand.
            buf.quad(
                [
                    Vec3::new(x0, y0, zf),
                    Vec3::new(x1, y0, zf),
                    Vec3::new(x1, y1, zf),
                    Vec3::new(x0, y1, zf),
                ],
                Vec3::Z,
                colour,
                TINT_FRONT,
            );

            // Sides, only where they'd actually be seen — the inside of a tunnel.
            if grid.is_air(xi, yi + 1) {
                buf.quad(
                    [
                        Vec3::new(x0, y1, zf),
                        Vec3::new(x1, y1, zf),
                        Vec3::new(x1, y1, zb),
                        Vec3::new(x0, y1, zb),
                    ],
                    Vec3::Y,
                    colour,
                    TINT_TOP,
                );
            }
            if grid.is_air(xi, yi - 1) {
                buf.quad(
                    [
                        Vec3::new(x0, y0, zb),
                        Vec3::new(x1, y0, zb),
                        Vec3::new(x1, y0, zf),
                        Vec3::new(x0, y0, zf),
                    ],
                    Vec3::NEG_Y,
                    colour,
                    TINT_BOTTOM,
                );
            }
            if grid.is_air(xi + 1, yi) {
                buf.quad(
                    [
                        Vec3::new(x1, y0, zf),
                        Vec3::new(x1, y0, zb),
                        Vec3::new(x1, y1, zb),
                        Vec3::new(x1, y1, zf),
                    ],
                    Vec3::X,
                    colour,
                    TINT_SIDE,
                );
            }
            if grid.is_air(xi - 1, yi) {
                buf.quad(
                    [
                        Vec3::new(x0, y0, zb),
                        Vec3::new(x0, y0, zf),
                        Vec3::new(x0, y1, zf),
                        Vec3::new(x0, y1, zb),
                    ],
                    Vec3::NEG_X,
                    colour,
                    TINT_SIDE,
                );
            }
        }
    }

    buf.into_mesh()
}

pub fn remesh_dirty_chunks(
    mut commands: Commands,
    mut grid: ResMut<SandGrid>,
    palette: Res<SandPalette>,
    mut meshes: ResMut<Assets<Mesh>>,
    chunks: Query<(Entity, &SandChunk, Option<&Mesh3d>)>,
) {
    for (entity, chunk, mesh3d) in &chunks {
        let c = chunk.cy * CHUNKS_X + chunk.cx;
        if !grid.dirty[c] {
            continue;
        }
        grid.dirty[c] = false;

        let mesh = build_chunk_mesh(&grid, &palette, chunk.cx, chunk.cy);

        // A chunk that's gone entirely to air just hides; its old geometry stays put,
        // unused, rather than being replaced with an empty upload.
        if mesh.count_vertices() == 0 {
            commands.entity(entity).insert(Visibility::Hidden);
            continue;
        }

        match mesh3d {
            // Only fails if the handle went stale, in which case the chunk is gone.
            Some(handle) => {
                let _ = meshes.insert(handle.id(), mesh);
            }
            // First sand to reach a chunk that started as open air.
            None => {
                commands.entity(entity).insert(Mesh3d(meshes.add(mesh)));
            }
        }
        commands.entity(entity).insert(Visibility::Inherited);
    }
}
