use core::ops::Neg;

use crate::{AlphaMode2d, MeshMaterial2d};
use bevy_app::{App, Plugin, Update};
use bevy_asset::{Assets, Handle};
use bevy_color::Color;
use bevy_derive::{Deref, DerefMut};
use bevy_ecs::{
    component::Component,
    entity::Entity,
    hierarchy::ChildOf,
    lifecycle::HookContext,
    query::Changed,
    reflect::{ReflectComponent, ReflectResource},
    relationship::Relationship,
    resource::Resource,
    system::{Commands, Query, ResMut},
    world::DeferredWorld,
};
use bevy_image::Image;
use bevy_math::{primitives::Rectangle, UVec2};
use bevy_mesh::{Mesh, Mesh2d};
use bevy_platform::collections::HashMap;
use bevy_reflect::{prelude::*, Reflect};
use bevy_sprite::{TileData, TileStorage, Tilemap};
use bevy_transform::components::Transform;
use bevy_utils::default;
use tracing::warn;

mod tilemap_chunk_material;

pub use tilemap_chunk_material::*;

/// Plugin that handles the initialization and updating of tilemap chunks.
/// Adds systems for processing newly added tilemap chunks and updating their indices.
pub struct TilemapChunkPlugin;

impl Plugin for TilemapChunkPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TilemapChunkMeshCache>()
            .add_systems(Update, update_tilemap_chunk_indices);
    }
}

/// A resource storing the meshes for each tilemap chunk size.
#[derive(Resource, Default, Deref, DerefMut, Reflect)]
#[reflect(Resource, Default)]
pub struct TilemapChunkMeshCache(HashMap<UVec2, Handle<Mesh>>);

/// Information for rendering chunks in a tilemap
#[derive(Component, Clone, Debug, Default, Reflect)]
#[reflect(Component, Clone, Debug, Default)]
#[component(immutable)]
#[require(Transform)]
pub struct TilemapChunkRenderer {
    /// Handle to the tileset image containing all tile textures.
    pub tileset: Handle<Image>,
    /// The alpha mode to use for the tilemap chunk.
    pub alpha_mode: AlphaMode2d,
}

/// Data for a single tile in the tilemap chunk.
#[derive(Clone, Copy, Debug, Reflect)]
#[reflect(Clone, Debug, Default)]
pub struct TileRenderData {
    /// The index of the tile in the corresponding tileset array texture.
    pub tileset_index: u16,
    /// The color tint of the tile. White leaves the sampled texture color unchanged.
    pub color: Color,
    /// The visibility of the tile.
    pub visible: bool,
}

impl TileRenderData {
    /// Creates a new `TileData` with the given tileset index and default values.
    pub fn from_tileset_index(tileset_index: u16) -> Self {
        Self {
            tileset_index,
            ..default()
        }
    }
}

impl TileData for TileRenderData {}

impl Default for TileRenderData {
    fn default() -> Self {
        Self {
            tileset_index: 0,
            color: Color::WHITE,
            visible: true,
        }
    }
}

fn update_tilemap_chunk_indices(
    query: Query<
        (
            Entity,
            &ChildOf,
            &TileStorage<TileRenderData>,
            Option<&MeshMaterial2d<TilemapChunkMaterial>>,
        ),
        Changed<TileStorage<TileRenderData>>,
    >,
    map_query: Query<(&Tilemap, &TilemapChunkRenderer)>,
    mut tilemap_chunk_mesh_cache: ResMut<TilemapChunkMeshCache>,
    mut materials: ResMut<Assets<TilemapChunkMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut commands: Commands,
) {
    for (chunk_id, in_map, storage, material) in query {
        let Ok((map, map_renderer)) = map_query.get(in_map.get()) else {
            warn!(
                "Could not find Tilemap {} for chunk {}",
                in_map.get(),
                chunk_id
            );
            continue;
        };

        let packed_tile_data: Vec<PackedTileData> =
            storage.tiles.iter().map(|&tile| tile.into()).collect();

        // Getting the material mutably to trigger change detection
        if let Some(material) = material.and_then(|material| materials.get_mut(material.id())) {
            let Some(tile_data_image) = images.get_mut(&material.tile_data) else {
                warn!(
                    "TilemapChunkMaterial tile data image not found for tilemap chunk {}",
                    chunk_id
                );
                continue;
            };
            let Some(data) = tile_data_image.data.as_mut() else {
                warn!(
                    "TilemapChunkMaterial tile data image data not found for tilemap chunk {}",
                    chunk_id
                );
                continue;
            };
            data.clear();
            data.extend_from_slice(bytemuck::cast_slice(&packed_tile_data));
        } else {
            let tile_data_image = make_chunk_tile_data_image(&storage.size, &packed_tile_data);

            let mesh_size = storage.size * map.tile_display_size;

            let mesh = if let Some(mesh) = tilemap_chunk_mesh_cache.get(&mesh_size) {
                mesh.clone()
            } else {
                let mesh = meshes.add(Rectangle::from_size(mesh_size.as_vec2()));
                tilemap_chunk_mesh_cache.insert(mesh_size, mesh.clone());
                mesh
            };
            let tile_data = images.add(tile_data_image);

            let material = materials.add(TilemapChunkMaterial {
                tileset: map_renderer.tileset.clone(),
                tile_data,
                alpha_mode: map_renderer.alpha_mode,
            });

            commands
                .entity(chunk_id)
                .insert((Mesh2d(mesh), MeshMaterial2d(material)));
        };
    }
}
