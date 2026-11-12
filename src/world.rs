use std::collections::{HashMap, HashSet};
use std::fmt;

use bevy::prelude::{Quat, Resource, Vec2, Vec3};

pub const CHUNK_SIZE: f32 = 10.0;
pub const WORLD_CHUNKS_PER_AXIS: i32 = 20;
pub const WORLD_SIZE: f32 = CHUNK_SIZE * WORLD_CHUNKS_PER_AXIS as f32;
pub const WORLD_HALF_SIZE: f32 = WORLD_SIZE / 2.0;
pub const MAX_OBJECTS_PER_CHUNK: usize = 32;
const GENERATED_TREE_ID_BASE: u32 = 10_000;
const TREE_ID_CHUNK_STRIDE: u32 = 200;
pub const RIVER_CHANNEL_CENTER_X_OFFSET: f32 = 0.8;
pub const RIVER_CHANNEL_WIDTH: f32 = 3.5;
pub const RIVER_CHANNEL_SURFACE_THICKNESS: f32 = 0.08;
pub const HILL_CENTER_X: f32 = -25.0;
pub const HILL_CENTER_Z: f32 = 5.0;
pub const HILL_RADIUS: f32 = 15.0;
pub const HILL_HEIGHT: f32 = 1.8;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ChunkCoord {
    pub x: i32,
    pub z: i32,
}

impl ChunkCoord {
    pub fn from_world_position(position: Vec3) -> Self {
        Self {
            x: chunk_index(position.x),
            z: chunk_index(position.z),
        }
    }

    pub fn center(self) -> Vec3 {
        Vec3::new(
            self.x as f32 * CHUNK_SIZE - WORLD_HALF_SIZE + CHUNK_SIZE / 2.0,
            0.0,
            self.z as f32 * CHUNK_SIZE - WORLD_HALF_SIZE + CHUNK_SIZE / 2.0,
        )
    }

    pub fn world_position(self, local_position: Vec3) -> Vec3 {
        self.center() + local_position
    }
}

fn wrapped_delta(from: f32, to: f32) -> f32 {
    (from - to + WORLD_HALF_SIZE).rem_euclid(WORLD_SIZE) - WORLD_HALF_SIZE
}

fn chunk_index(coordinate: f32) -> i32 {
    (((coordinate + WORLD_HALF_SIZE) / CHUNK_SIZE).floor() as i32).rem_euclid(WORLD_CHUNKS_PER_AXIS)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerrainKind {
    Flat,
    RiverChannel,
    GentleSlope,
}

/// Stable identity for one placed world object. Existing values must not be reassigned.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ObjectInstanceId(pub u32);

pub(crate) const BRIDGE_SITE_ID: ObjectInstanceId = ObjectInstanceId(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObjectKind {
    Log,
    Bridge,
    Tree,
    AxePickup,
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(dead_code)]
pub enum ColliderShape {
    Box { half_extents: Vec3 },
    Cylinder { radius: f32, half_height: f32 },
    Cone { radius: f32, half_height: f32 },
}

impl ColliderShape {
    pub fn horizontal_radius(self) -> f32 {
        match self {
            Self::Box { half_extents } => Vec2::new(half_extents.x, half_extents.z).length(),
            Self::Cylinder { radius, .. } | Self::Cone { radius, .. } => radius,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WorldFlag {
    BridgeBuilt,
}

#[derive(Clone, Copy, Debug)]
pub struct ObjectPlacement {
    pub id: ObjectInstanceId,
    pub kind: ObjectKind,
    pub local_position: Vec3,
    pub rotation: Quat,
    pub collider: ColliderShape,
    pub completion_flag: Option<WorldFlag>,
}

#[derive(Debug)]
pub struct ChunkDefinition {// チャンクの定義
    pub terrain: TerrainKind,
    pub objects: Vec<ObjectPlacement>,
}

#[derive(Resource)]
pub struct WorldMap {// ワールドマップの定義
    pub chunks: HashMap<ChunkCoord, ChunkDefinition>,
}

#[derive(Debug, Eq, PartialEq)]
pub enum WorldMapValidationError {
    ChunkOutsideWorld(ChunkCoord),
    MissingChunk(ChunkCoord),
    TooManyObjects {
        chunk: ChunkCoord,
        count: usize,
    },
    InvalidObjectId(ObjectInstanceId),
    DuplicateObjectId(ObjectInstanceId),
    InvalidObjectPosition {
        chunk: ChunkCoord,
        object: ObjectInstanceId,
    },
    ObjectOutsideOwningChunk {
        chunk: ChunkCoord,
        object: ObjectInstanceId,
    },
    InvalidObjectRotation(ObjectInstanceId),
    InvalidObjectCollider(ObjectInstanceId),
}

impl fmt::Display for WorldMapValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ChunkOutsideWorld(chunk) => {
                write!(
                    formatter,
                    "チャンク座標 ({}, {}) がワールド範囲外です",
                    chunk.x, chunk.z
                )
            }
            Self::MissingChunk(chunk) => {
                write!(
                    formatter,
                    "チャンク座標 ({}, {}) の定義がありません",
                    chunk.x, chunk.z
                )
            }
            Self::TooManyObjects { chunk, count } => write!(
                formatter,
                "チャンク ({}, {}) の配置数 {count} が上限 {MAX_OBJECTS_PER_CHUNK} を超えています",
                chunk.x, chunk.z
            ),
            Self::InvalidObjectId(id) => {
                write!(formatter, "配置オブジェクトID {} は予約済みです", id.0)
            }
            Self::DuplicateObjectId(id) => {
                write!(formatter, "配置オブジェクトID {} が重複しています", id.0)
            }
            Self::InvalidObjectPosition { chunk, object } => write!(
                formatter,
                "配置オブジェクトID {} の位置が有限値ではありません（チャンク {}, {}）",
                object.0, chunk.x, chunk.z
            ),
            Self::ObjectOutsideOwningChunk { chunk, object } => write!(
                formatter,
                "配置オブジェクトID {} が所有チャンクから離れすぎています（チャンク {}, {}）",
                object.0, chunk.x, chunk.z
            ),
            Self::InvalidObjectRotation(id) => {
                write!(formatter, "配置オブジェクトID {} の回転が不正です", id.0)
            }
            Self::InvalidObjectCollider(id) => {
                write!(
                    formatter,
                    "配置オブジェクトID {} のColliderが不正です",
                    id.0
                )
            }
        }
    }
}

impl std::error::Error for WorldMapValidationError {}

impl WorldMap {// ワールドマップの定義に関するメソッド群
    pub fn validate(&self) -> Result<(), Vec<WorldMapValidationError>> {
        let mut errors = Vec::new();
        let mut object_ids = HashSet::new();

        for (&chunk_coord, chunk) in &self.chunks {
            if !is_chunk_coord_in_world(chunk_coord) {
                errors.push(WorldMapValidationError::ChunkOutsideWorld(chunk_coord));
            }

            if chunk.objects.len() > MAX_OBJECTS_PER_CHUNK {
                errors.push(WorldMapValidationError::TooManyObjects {
                    chunk: chunk_coord,
                    count: chunk.objects.len(),
                });
            }

            for object in &chunk.objects {
                if object.id.0 == 0 {
                    errors.push(WorldMapValidationError::InvalidObjectId(object.id));
                }
                if !object_ids.insert(object.id) {
                    errors.push(WorldMapValidationError::DuplicateObjectId(object.id));
                }

                if !object.local_position.is_finite() {
                    errors.push(WorldMapValidationError::InvalidObjectPosition {
                        chunk: chunk_coord,
                        object: object.id,
                    });
                } else if collider_is_valid(object.collider) {
                    let allowed_offset = CHUNK_SIZE / 2.0 + object.collider.horizontal_radius();
                    if object.local_position.x < -allowed_offset
                        || object.local_position.x > allowed_offset
                        || object.local_position.z < -allowed_offset
                        || object.local_position.z > allowed_offset
                    {
                        errors.push(WorldMapValidationError::ObjectOutsideOwningChunk {
                            chunk: chunk_coord,
                            object: object.id,
                        });
                    }
                }

                let rotation_length_squared = object.rotation.length_squared();
                if !object.rotation.is_finite() || (rotation_length_squared - 1.0).abs() > 0.001 {
                    errors.push(WorldMapValidationError::InvalidObjectRotation(object.id));
                }

                if !collider_is_valid(object.collider)
                    || (matches!(object.kind, ObjectKind::Log | ObjectKind::Bridge)
                        && !matches!(object.collider, ColliderShape::Box { .. }))
                {
                    errors.push(WorldMapValidationError::InvalidObjectCollider(object.id));
                }
            }
        }

        for z in 0..WORLD_CHUNKS_PER_AXIS {
            for x in 0..WORLD_CHUNKS_PER_AXIS {
                let chunk_coord = ChunkCoord { x, z };
                if !self.chunks.contains_key(&chunk_coord) {
                    errors.push(WorldMapValidationError::MissingChunk(chunk_coord));
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    pub fn blocks_movement(&self, position: Vec3, bridge_built: bool, actor_radius: f32) -> bool {
        // 指定された座標が河川チャンクの範囲内にあり、橋が完成していない場合は移動をブロックする
        let coord = ChunkCoord::from_world_position(position);
        let Some(chunk) = self.chunks.get(&coord) else {
            return true;
        };

        if chunk.terrain != TerrainKind::RiverChannel {
            return false;
        }

        let river_center_x = coord.center().x + RIVER_CHANNEL_CENTER_X_OFFSET;
        let river_half_width = RIVER_CHANNEL_WIDTH / 2.0;
        let overlaps_river =
            wrapped_delta(position.x, river_center_x).abs() <= river_half_width + actor_radius;
        overlaps_river && !(bridge_built && self.bridge_covers(position, actor_radius))
    }

    pub fn ground_height(&self, x: f32, z: f32) -> f32 {
        let hill_distance = Vec2::new(
            wrapped_delta(x, HILL_CENTER_X) / HILL_RADIUS,
            wrapped_delta(z, HILL_CENTER_Z) / HILL_RADIUS,
        )
        .length();
        if hill_distance >= 1.0 {
            return 0.0;
        }

        HILL_HEIGHT * (0.5 + 0.5 * (std::f32::consts::PI * hill_distance).cos())
    }

    fn bridge_covers(&self, position: Vec3, actor_radius: f32) -> bool {
        // 指定された座標が橋のコライダーの範囲内にあるかどうかを判定する
        let center = ChunkCoord::from_world_position(position);
        for z_offset in -1..=1 {
            for x_offset in -1..=1 {
                let coord = ChunkCoord {
                    x: (center.x + x_offset).rem_euclid(WORLD_CHUNKS_PER_AXIS),
                    z: (center.z + z_offset).rem_euclid(WORLD_CHUNKS_PER_AXIS),
                };
                let Some(chunk) = self.chunks.get(&coord) else {
                    continue;
                };

                for bridge in chunk
                    .objects
                    .iter()
                    .filter(|object| object.kind == ObjectKind::Bridge)
                {
                    let bridge_position = coord.world_position(bridge.local_position);
                    let delta = Vec3::new(
                        wrapped_delta(position.x, bridge_position.x),
                        0.0,
                        wrapped_delta(position.z, bridge_position.z),
                    );
                    let local_delta = bridge.rotation.inverse() * delta;
                    if let ColliderShape::Box { half_extents } = bridge.collider {
                        if local_delta.x.abs() <= half_extents.x + actor_radius
                            && local_delta.z.abs() <= half_extents.z + actor_radius
                        {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    pub fn prototype() -> Self {// ワールドマップのプロトタイプを生成する
        let mut chunks =
            HashMap::with_capacity((WORLD_CHUNKS_PER_AXIS * WORLD_CHUNKS_PER_AXIS) as usize);

        for z in 0..WORLD_CHUNKS_PER_AXIS {
            for x in 0..WORLD_CHUNKS_PER_AXIS {
                let coord = ChunkCoord { x, z };
                let terrain = if x == 10 {
                    TerrainKind::RiverChannel
                } else if (6..=8).contains(&x) && (8..=11).contains(&z) {
                    TerrainKind::GentleSlope
                } else {
                    TerrainKind::Flat
                };
                chunks.insert(
                    coord,
                    ChunkDefinition {
                        terrain,
                        objects: Vec::new(),
                    },
                );

                let placement_seed = (x * 37 + z * 71 + x * z * 13).rem_euclid(97);
                if terrain != TerrainKind::RiverChannel && placement_seed % 7 == 0 {
                    let chunk = chunks.get_mut(&coord).expect("chunk was just inserted");
                    let local_x = (placement_seed * 17).rem_euclid(81) as f32 / 10.0 - 4.0;
                    let local_z = (placement_seed * 29 + 11).rem_euclid(81) as f32 / 10.0 - 4.0;
                    chunk.objects.push(ObjectPlacement {
                        id: prototype_tree_id(coord),
                        kind: ObjectKind::Tree,
                        local_position: Vec3::new(local_x, 0.0, local_z),
                        rotation: Quat::IDENTITY,
                        collider: ColliderShape::Cylinder {
                            radius: 0.35,
                            half_height: 1.5,
                        },
                        completion_flag: None,
                    });
                }
            }
        }

        let center = chunks
            .get_mut(&ChunkCoord { x: 10, z: 10 })
            .expect("the prototype map contains its center chunk");
        center.objects.extend([
            ObjectPlacement {
                id: ObjectInstanceId(500),
                kind: ObjectKind::AxePickup,
                local_position: Vec3::new(-4.0, 0.45, -4.0),
                rotation: Quat::IDENTITY,
                collider: ColliderShape::Box {
                    half_extents: Vec3::new(0.35, 0.45, 0.25),
                },
                completion_flag: None,
            },
            ObjectPlacement {
                id: BRIDGE_SITE_ID,
                kind: ObjectKind::Log,
                local_position: Vec3::new(-2.0, 0.18, -4.0),
                rotation: Quat::IDENTITY,
                collider: ColliderShape::Box {
                    half_extents: Vec3::new(0.8, 0.18, 0.225),
                },
                completion_flag: Some(WorldFlag::BridgeBuilt),
            },
            ObjectPlacement {
                id: ObjectInstanceId(2),
                kind: ObjectKind::Bridge,
                local_position: Vec3::new(0.8, 0.2, -5.0),
                rotation: Quat::IDENTITY,
                collider: ColliderShape::Box {
                    half_extents: Vec3::new(2.1, 0.11, 0.75),
                },
                completion_flag: None,
            },
        ]);
        assert!(center.objects.len() <= MAX_OBJECTS_PER_CHUNK);

        let nearby_tree = chunks
            .get_mut(&ChunkCoord { x: 9, z: 10 })
            .expect("the prototype map contains the chunk west of the river");
        nearby_tree.objects.push(ObjectPlacement {
            id: ObjectInstanceId(501),
            kind: ObjectKind::Tree,
            local_position: Vec3::new(3.0, 0.0, -5.0),
            rotation: Quat::IDENTITY,
            collider: ColliderShape::Cylinder {
                radius: 0.4,
                half_height: 1.5,
            },
            completion_flag: None,
        });

        Self { chunks }
    }
}

fn is_chunk_coord_in_world(coord: ChunkCoord) -> bool {
    (0..WORLD_CHUNKS_PER_AXIS).contains(&coord.x) && (0..WORLD_CHUNKS_PER_AXIS).contains(&coord.z)
}

fn prototype_tree_id(coord: ChunkCoord) -> ObjectInstanceId {
    ObjectInstanceId(
        GENERATED_TREE_ID_BASE + coord.z as u32 * TREE_ID_CHUNK_STRIDE + coord.x as u32,
    )
}

fn collider_is_valid(collider: ColliderShape) -> bool {
    match collider {
        ColliderShape::Box { half_extents } => {
            half_extents.is_finite()
                && half_extents.x > 0.0
                && half_extents.y > 0.0
                && half_extents.z > 0.0
        }
        ColliderShape::Cylinder {
            radius,
            half_height,
        }
        | ColliderShape::Cone {
            radius,
            half_height,
        } => radius.is_finite() && radius > 0.0 && half_height.is_finite() && half_height > 0.0,
    }
}

#[cfg(test)]
mod tests {// テスト用のモジュール
    use super::*;

    #[test]
    fn prototype_has_twenty_by_twenty_chunks() {
        // プロトタイプのワールドマップが20x20のチャンクを持つことを確認する
        assert_eq!(WorldMap::prototype().chunks.len(), 400);
    }

    #[test]
    fn prototype_world_map_passes_definition_validation() {
        assert_eq!(WorldMap::prototype().validate(), Ok(()));
    }

    #[test]
    fn gentle_slope_rises_smoothly_and_returns_to_flat_ground() {
        let map = WorldMap::prototype();
        let center_height = map.ground_height(HILL_CENTER_X, HILL_CENTER_Z);
        let halfway_height =
            map.ground_height(HILL_CENTER_X + HILL_RADIUS / 2.0, HILL_CENTER_Z);

        assert_eq!(center_height, HILL_HEIGHT);
        assert!((halfway_height - HILL_HEIGHT / 2.0).abs() < 0.001);
        assert_eq!(
            map.ground_height(HILL_CENTER_X + HILL_RADIUS, HILL_CENTER_Z),
            0.0
        );
        assert_eq!(map.ground_height(0.0, 0.0), 0.0);
    }

    #[test]
    fn gentle_slope_height_is_continuous_across_chunk_boundaries() {
        let map = WorldMap::prototype();
        let boundary_x = -30.0;
        let boundary_z = 0.0;
        let left_height = map.ground_height(boundary_x - 0.001, boundary_z);
        let right_height = map.ground_height(boundary_x + 0.001, boundary_z);

        assert!((left_height - right_height).abs() < 0.001);
        assert_eq!(
            map.chunks[&ChunkCoord { x: 7, z: 10 }].terrain,
            TerrainKind::GentleSlope
        );
    }

    #[test]
    fn validation_reports_duplicate_object_ids() {
        let mut map = WorldMap::prototype();
        let duplicate_id = {
            let center = map
                .chunks
                .get_mut(&ChunkCoord { x: 10, z: 10 })
                .expect("prototype map has its center chunk");
            let duplicate_id = center.objects[0].id;
            center.objects[1].id = duplicate_id;
            duplicate_id
        };

        assert!(map.validate().is_err_and(|errors| {
            errors.contains(&WorldMapValidationError::DuplicateObjectId(duplicate_id))
        }));
    }

    #[test]
    fn validation_reports_invalid_positions_and_colliders() {
        let mut map = WorldMap::prototype();
        {
            let center = map
                .chunks
                .get_mut(&ChunkCoord { x: 10, z: 10 })
                .expect("prototype map has its center chunk");
            center.objects[0].local_position.x = f32::NAN;
            center.objects[1].collider = ColliderShape::Box {
                half_extents: Vec3::new(0.0, 0.2, 0.2),
            };
        }

        let errors = map
            .validate()
            .expect_err("invalid placement data is rejected");
        assert!(
            errors.iter().any(|error| matches!(
                error,
                WorldMapValidationError::InvalidObjectPosition { .. }
            ))
        );
        assert!(
            errors
                .iter()
                .any(|error| matches!(error, WorldMapValidationError::InvalidObjectCollider(_)))
        );
    }

    #[test]
    fn validation_requires_box_colliders_for_bridge_and_build_site() {
        let mut map = WorldMap::prototype();
        let center = map
            .chunks
            .get_mut(&ChunkCoord { x: 10, z: 10 })
            .expect("prototype map has its center chunk");
        for object in &mut center.objects {
            if matches!(object.kind, ObjectKind::Log | ObjectKind::Bridge) {
                object.collider = ColliderShape::Cylinder {
                    radius: 0.5,
                    half_height: 0.5,
                };
            }
        }

        let errors = map
            .validate()
            .expect_err("bridge and build site require box colliders");
        assert!(errors.contains(&WorldMapValidationError::InvalidObjectCollider(
            ObjectInstanceId(1)
        )));
        assert!(errors.contains(&WorldMapValidationError::InvalidObjectCollider(
            ObjectInstanceId(2)
        )));
    }

    #[test]
    fn validation_reports_reserved_ids_invalid_rotations_and_out_of_world_chunks() {
        let mut map = WorldMap::prototype();
        {
            let center = map
                .chunks
                .get_mut(&ChunkCoord { x: 10, z: 10 })
                .expect("prototype map has its center chunk");
            center.objects[0].id = ObjectInstanceId(0);
            center.objects[1].rotation = Quat::from_xyzw(0.0, 0.0, 0.0, 2.0);
        }
        map.chunks.insert(
            ChunkCoord {
                x: WORLD_CHUNKS_PER_AXIS,
                z: 0,
            },
            ChunkDefinition {
                terrain: TerrainKind::Flat,
                objects: Vec::new(),
            },
        );

        let errors = map
            .validate()
            .expect_err("invalid IDs and coordinates are rejected");
        assert!(
            errors.contains(&WorldMapValidationError::InvalidObjectId(ObjectInstanceId(
                0
            )))
        );
        assert!(
            errors.contains(&WorldMapValidationError::InvalidObjectRotation(
                ObjectInstanceId(1)
            ))
        );
        assert!(
            errors.contains(&WorldMapValidationError::ChunkOutsideWorld(ChunkCoord {
                x: WORLD_CHUNKS_PER_AXIS,
                z: 0,
            }))
        );
    }

    #[test]
    fn validation_reports_missing_chunks_and_object_limit_violations() {
        let mut map = WorldMap::prototype();
        map.chunks.remove(&ChunkCoord { x: 0, z: 0 });
        {
            let center = map
                .chunks
                .get_mut(&ChunkCoord { x: 10, z: 10 })
                .expect("prototype map has its center chunk");
            let object = center.objects[0];
            center.objects.resize(MAX_OBJECTS_PER_CHUNK + 1, object);
        }

        let errors = map
            .validate()
            .expect_err("invalid map structure is rejected");
        assert!(
            errors.contains(&WorldMapValidationError::MissingChunk(ChunkCoord {
                x: 0,
                z: 0,
            }))
        );
        assert!(
            errors
                .iter()
                .any(|error| matches!(error, WorldMapValidationError::TooManyObjects { .. }))
        );
    }

    #[test]
    fn validation_allows_boundary_spanning_objects_but_rejects_distant_placements() {
        let mut map = WorldMap::prototype();
        {
            let center = map
                .chunks
                .get_mut(&ChunkCoord { x: 10, z: 10 })
                .expect("prototype map has its center chunk");
            center.objects[0].local_position.x = 100.0;
        }

        assert!(map.validate().is_err_and(|errors| {
            errors.iter().any(|error| {
                matches!(
                    error,
                    WorldMapValidationError::ObjectOutsideOwningChunk { .. }
                )
            })
        }));
    }

    #[test]
    fn world_positions_wrap_to_chunk_coordinates() {
        // ワールド座標がチャンク座標に正しく変換されることを確認する
        assert_eq!(
            ChunkCoord::from_world_position(Vec3::new(-WORLD_HALF_SIZE, 0.0, -WORLD_HALF_SIZE)),
            ChunkCoord { x: 0, z: 0 }
        );
        assert_eq!(
            ChunkCoord::from_world_position(Vec3::new(WORLD_HALF_SIZE, 0.0, WORLD_HALF_SIZE)),
            ChunkCoord { x: 0, z: 0 }
        );
    }

    #[test]
    fn prototype_object_count_is_below_per_chunk_limit() {
        // プロトタイプのワールドマップの各チャンクが定義された最大オブジェクト数を超えないことを確認する
        let map = WorldMap::prototype();
        let center = map.chunks.get(&ChunkCoord { x: 10, z: 10 }).unwrap();
        assert!(center.objects.len() <= MAX_OBJECTS_PER_CHUNK);
    }

    #[test]
    fn chunk_local_positions_are_offset_from_chunk_center() {
        // チャンクのローカル位置がチャンク中心から正しくオフセットされていることを確認する
        let chunk = ChunkCoord { x: 10, z: 10 };
        let world_position = chunk.world_position(Vec3::new(-2.0, 0.18, -5.0));
        assert_eq!(world_position, Vec3::new(3.0, 0.18, 0.0));
    }

    #[test]
    fn trees_are_sparse_and_never_placed_in_river_chunks() {
        // 木は疎で河川チャンクには配置されず、地形変更後も既存の木IDを保つことを確認する
        let map = WorldMap::prototype();
        let trees: Vec<_> = map
            .chunks
            .values()
            .flat_map(|chunk| {
                chunk
                    .objects
                    .iter()
                    .filter(|object| object.kind == ObjectKind::Tree)
                    .map(move |object| (chunk, object))
            })
            .collect();

        assert!(!trees.is_empty());
        assert!(trees.len() < 100);
        assert!(trees.iter().all(|(chunk, object)| chunk.terrain != TerrainKind::RiverChannel
            && matches!(object.collider, ColliderShape::Cylinder { .. })));
        assert!(trees
            .iter()
            .any(|(_, object)| object.id == ObjectInstanceId(11_606)));
    }

    #[test]
    fn river_blocks_movement_until_the_defined_bridge_is_built() {
        // 河川チャンクが定義された橋が完成するまで移動をブロックすることを確認する
        let map = WorldMap::prototype();
        let river_position = Vec3::new(5.8, 0.0, 0.0);

        assert!(map.blocks_movement(river_position, false, 0.4));
        assert!(!map.blocks_movement(river_position, true, 0.4));
        assert!(map.blocks_movement(Vec3::new(5.8, 0.0, 2.0), true, 0.4));
        assert!(!map.blocks_movement(Vec3::new(8.0, 0.0, 0.0), false, 0.4));
    }

    #[test]
    fn river_collision_uses_the_configured_width_and_center() {
        // 河川の衝突判定が設定された幅と中心を使用していることを確認する
        let map = WorldMap::prototype();
        let river_center_x = ChunkCoord { x: 10, z: 10 }.center().x + RIVER_CHANNEL_CENTER_X_OFFSET;
        let actor_radius = 0.4;
        let collision_edge_x = river_center_x + RIVER_CHANNEL_WIDTH / 2.0 + actor_radius;

        assert!(map.blocks_movement(
            Vec3::new(collision_edge_x - 0.01, 0.0, 0.0),
            false,
            actor_radius,
        ));
        assert!(!map.blocks_movement(
            Vec3::new(collision_edge_x + 0.01, 0.0, 0.0),
            false,
            actor_radius,
        ));
    }
}
