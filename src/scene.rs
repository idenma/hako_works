// scene.rs
use std::collections::{HashMap, HashSet};

use bevy::asset::{RenderAssetUsages, UntypedAssetLoadFailedEvent};
use bevy::gltf::GltfAssetLabel;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::text::FontSize;
use bevy::world_serialization::{WorldAsset, WorldAssetRoot};

use crate::inventory::{InventoryHud, ItemKind};
use crate::player::{FollowCamera, PLAYER_GROUND_Y, Player};
use crate::world::{
    CHUNK_SIZE, ChunkCoord, ChunkDefinition, ColliderShape, ObjectInstanceId, ObjectKind,
    ObjectPlacement, RIVER_CHANNEL_CENTER_X_OFFSET, RIVER_CHANNEL_SURFACE_THICKNESS,
    RIVER_CHANNEL_WIDTH, TerrainKind, WORLD_CHUNKS_PER_AXIS, WorldFlag, WorldMap,
};

const CHUNK_LOAD_RADIUS: i32 = 1;
const TERRAIN_MESH_SUBDIVISIONS: usize = 16;
const NPC_START_X: f32 = 3.2;
const NPC_DIALOGUE_DISTANCE: f32 = 6.0;

#[derive(Component)]
pub(crate) struct Interactable {
    pub(crate) action: InteractionAction,
}

#[derive(Component, Clone, Copy)]
pub(crate) enum InteractionAction {
    PickUp(ItemKind),
    ChopTree,
    BuildBridge(WorldFlag),
}

#[derive(Component)]
pub(crate) struct InteractionPopup;

#[derive(Component)]
pub(crate) struct InteractionPrompt;

#[derive(Component)]
pub(crate) struct BridgePart;

#[derive(Component)]
pub(crate) struct ChunkEntity(ChunkCoord);

#[derive(Component)]
pub(crate) struct ObjectCollider(pub(crate) ColliderShape);

#[derive(Component)]
pub(crate) struct ObjectEntity(pub(crate) ObjectInstanceId);

#[derive(Component)]
struct RiverChannel;

#[derive(Component)]
pub(crate) struct RiversideNpc;

#[derive(Component)]
pub(crate) struct NpcDialogueHud;

#[derive(Resource)]
pub(crate) struct WorldRenderAssets {
    ground_mesh: Handle<Mesh>,
    river_mesh: Handle<Mesh>,
    popup_mesh: Handle<Mesh>,
    tree_scene: Handle<WorldAsset>,
    axe_scene: Handle<WorldAsset>,
    log_scene: Handle<WorldAsset>,
    bridge_deck_scene: Handle<WorldAsset>,
    bridge_rail_scene: Handle<WorldAsset>,
    player_scene: Handle<WorldAsset>,
    npc_scene: Handle<WorldAsset>,
    ground_material: Handle<StandardMaterial>,
    river_material: Handle<StandardMaterial>,
    object_material: Handle<StandardMaterial>,
}

#[derive(Resource, Default)]
pub(crate) struct WorldFlags(HashMap<WorldFlag, bool>);

#[derive(Resource, Default)]
pub(crate) struct WorldProgress {
    pub(crate) collected_objects: HashSet<ObjectInstanceId>,
    pub(crate) chopped_trees: HashSet<ObjectInstanceId>,
}

impl WorldFlags {
    pub(crate) fn is_set(&self, flag: WorldFlag) -> bool {
        self.0.get(&flag).copied().unwrap_or(false)
    }

    pub(crate) fn set(&mut self, flag: WorldFlag, value: bool) {
        self.0.insert(flag, value);
    }
}

#[derive(Message)]
pub(crate) struct FlagChanged {
    pub(crate) flag: WorldFlag,
    pub(crate) value: bool,
}

pub(crate) fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
) {
    commands.spawn((
        DirectionalLight::default(),
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.8, -0.5, 0.0)),
    ));

    let render_assets = WorldRenderAssets {// 描画用のアセットをロードする
        ground_mesh: meshes.add(Plane3d::default().mesh().size(CHUNK_SIZE, CHUNK_SIZE)),
        river_mesh: meshes.add(Cuboid::new(
            RIVER_CHANNEL_WIDTH,
            RIVER_CHANNEL_SURFACE_THICKNESS,
            CHUNK_SIZE,
        )),
        popup_mesh: meshes.add(Cuboid::new(0.35, 0.35, 0.35)),
        tree_scene: asset_server
            .load::<WorldAsset>(GltfAssetLabel::Scene(0).from_asset("3D_Object/tree_oak.glb")),
        axe_scene: asset_server
            .load::<WorldAsset>(GltfAssetLabel::Scene(0).from_asset("3D_Object/axe_pickup.glb")),
        log_scene: asset_server
            .load::<WorldAsset>(GltfAssetLabel::Scene(0).from_asset("3D_Object/bridge_log.glb")),
        bridge_deck_scene: asset_server
            .load::<WorldAsset>(GltfAssetLabel::Scene(0).from_asset("3D_Object/bridge_deck.glb")),
        bridge_rail_scene: asset_server
            .load::<WorldAsset>(GltfAssetLabel::Scene(0).from_asset("3D_Object/bridge_rail.glb")),
        player_scene: asset_server
            .load::<WorldAsset>(GltfAssetLabel::Scene(0).from_asset("3D_Object/player.glb")),
        npc_scene: asset_server
            .load::<WorldAsset>(GltfAssetLabel::Scene(0).from_asset("3D_Object/riverside_npc.glb")),
        ground_material: materials.add(Color::srgb(0.35, 0.55, 0.32)),
        river_material: materials.add(Color::srgb(0.12, 0.55, 0.78)),
        object_material: materials.add(Color::srgb(0.48, 0.27, 0.12)),
    };
    commands.spawn((// プレイヤーのEntityを生成する
        Player::default(),
        WorldAssetRoot(render_assets.player_scene.clone()),
        Transform::from_xyz(0.0, PLAYER_GROUND_Y, 0.0),
        Visibility::Inherited,
    ));

    // カメラの向きは固定し、位置だけをプレイヤーに追従させる。
    commands.spawn((
        FollowCamera,
        Camera3d::default(),
        Transform::from_xyz(0.0, 3.8, 8.0).looking_at(Vec3::new(0.0, 0.6, -3.0), Vec3::Y),
    ));

    commands.spawn((// 所持品HUDのEntityを生成する
        InventoryHud,
        Text::new("もちもの\n1 おの  0\n2 木ざい 0\n1/2 えらぶ"),
        TextFont {
            font: asset_server.load("fonts/mgenplus-1p-regular.ttf").into(),
            font_size: FontSize::Px(22.0),
            ..default()
        },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            left: px(18),
            top: px(18),
            padding: UiRect::all(px(12)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.04, 0.08, 0.08, 0.72)),
    ));

    commands.spawn((// 操作案内HUDのEntityを生成する
        InteractionPrompt,
        Text::new(""),
        TextFont {
            font: asset_server.load("fonts/mgenplus-1p-regular.ttf").into(),
            font_size: FontSize::Px(24.0),
            ..default()
        },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            left: px(18),
            bottom: px(18),
            padding: UiRect::axes(px(16), px(10)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.04, 0.08, 0.08, 0.82)),
        Visibility::Hidden,
    ));

    commands.spawn((// NPCの会話HUDのEntityを生成する
        NpcDialogueHud,
        Text::new(""),
        TextFont {
            font: asset_server.load("fonts/mgenplus-1p-regular.ttf").into(),
            font_size: FontSize::Px(24.0),
            ..default()
        },
        TextColor(Color::WHITE),
        TextLayout::justify(Justify::Center),
        Node {
            position_type: PositionType::Absolute,
            left: px(18),
            right: px(18),
            bottom: px(82),
            padding: UiRect::axes(px(18), px(12)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.04, 0.08, 0.08, 0.82)),
        Visibility::Hidden,
    ));

    spawn_riverside_npc(&mut commands, &render_assets);
    commands.insert_resource(render_assets);
}

pub(crate) fn validate_required_entities(// 必須のEntityが存在することを確認する
    players: Query<(), With<Player>>,
    cameras: Query<(), With<FollowCamera>>,
    inventory_huds: Query<(), With<InventoryHud>>,
    interaction_prompts: Query<(), With<InteractionPrompt>>,
    npcs: Query<(), With<RiversideNpc>>,
    dialogue_huds: Query<(), With<NpcDialogueHud>>,
) {
    report_singleton_entity("プレイヤー", players.iter().count());
    report_singleton_entity("カメラ", cameras.iter().count());
    report_singleton_entity("所持品HUD", inventory_huds.iter().count());
    report_singleton_entity("操作案内HUD", interaction_prompts.iter().count());
    report_singleton_entity("川辺のNPC", npcs.iter().count());
    report_singleton_entity("NPCの会話HUD", dialogue_huds.iter().count());
}

fn report_singleton_entity(name: &str, count: usize) {// 指定されたEntityが1つだけ存在することを確認する
    if count != 1 {
        error!("{name} Entityは1つ必要ですが、{count}個あります");
    }
}

pub(crate) fn log_asset_load_failures(mut failures: MessageReader<UntypedAssetLoadFailedEvent>) {
    // アセットの読み込みに失敗した場合、エラーログを出力する
    for failure in failures.read() {
        error!(
            "アセットの読み込みに失敗しました: {} ({})",
            failure.path, failure.error
        );
    }
}

fn spawn_riverside_npc(commands: &mut Commands, assets: &WorldRenderAssets) {
    // 川辺のNPCのEntityを生成する
    commands.spawn((
        RiversideNpc,
        WorldAssetRoot(assets.npc_scene.clone()),
        Transform::from_xyz(NPC_START_X, 0.0, 0.0).with_rotation(npc_facing(Vec3::X)),
        Visibility::Inherited,
    ));
}

pub(crate) fn update_riverside_npc(// 川辺のNPCの会話HUDを更新する
    flags: Res<WorldFlags>,
    players: Query<&Transform, (With<Player>, Without<RiversideNpc>)>,
    npcs: Query<&Transform, (With<RiversideNpc>, Without<Player>)>,
    mut dialogue_huds: Query<(&mut Text, &mut Visibility), With<NpcDialogueHud>>,
) {
    let (Ok(player_transform), Ok(npc_transform), Ok((mut text, mut visibility))) =
        (players.single(), npcs.single(), dialogue_huds.single_mut())
    else {
        return;
    };

    text.0 = riverside_npc_line(flags.is_set(WorldFlag::BridgeBuilt)).to_owned();
    *visibility = if is_near_npc(player_transform.translation, npc_transform.translation) {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
}

fn is_near_npc(player_position: Vec3, npc_position: Vec3) -> bool {
    // プレイヤーがNPCの会話範囲内にいるかどうかを判定する
    let offset = player_position - npc_position;
    Vec2::new(offset.x, offset.z).length_squared() <= NPC_DIALOGUE_DISTANCE.powi(2)
}

fn riverside_npc_line(bridge_built: bool) -> &'static str {// 川辺のNPCの会話内容を返す
    if bridge_built {
        "ありがとう！"
    } else {
        "かわを わたりたいな"
    }
}

fn npc_facing(direction: Vec3) -> Quat {// NPCが指定された方向を向くように回転を計算する
    Transform::IDENTITY.looking_to(direction, Vec3::Y).rotation
}

/// 現在地周辺のチャンクだけをEntity化し、範囲外のチャンクを破棄する。
pub(crate) fn sync_loaded_chunks(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    world_map: Res<WorldMap>,
    render_assets: Res<WorldRenderAssets>,
    flags: Res<WorldFlags>,
    progress: Res<WorldProgress>,
    players: Query<&Transform, With<Player>>,
    loaded_chunks: Query<(Entity, &ChunkEntity)>,
) {
    let Ok(player_transform) = players.single() else {
        // プレイヤーが存在しない場合はチャンクの同期を行わない
        return;
    };

    let center = ChunkCoord::from_world_position(player_transform.translation);
    let desired = nearby_chunk_coords(center);

    let loaded: HashSet<_> = loaded_chunks.iter().map(|(_, chunk)| chunk.0).collect();

    for (entity, chunk) in &loaded_chunks {
        if !desired.contains(&chunk.0) {
            // ChildOfのlinked_spawnにより、このチャンク配下も一緒に破棄される。
            commands.entity(entity).despawn();
        }
    }

    for world_coord in desired.difference(&loaded).copied() {
        let definition_coord = definition_chunk_coord(world_coord);
        if let Some(definition) = world_map.chunks.get(&definition_coord) {
            spawn_chunk(
                &mut commands,
                &mut meshes,
                &render_assets,
                &world_map,
                world_coord,
                definition,
                &flags,
                &progress,
            );
        }
    }
}

pub(crate) fn nearby_chunk_coords(center: ChunkCoord) -> HashSet<ChunkCoord> {
    // 中心チャンクの周囲3x3のチャンク座標を返す
    let diameter = (CHUNK_LOAD_RADIUS * 2 + 1) as usize;
    let mut coords = HashSet::with_capacity(diameter * diameter);
    for z_offset in -CHUNK_LOAD_RADIUS..=CHUNK_LOAD_RADIUS {
        for x_offset in -CHUNK_LOAD_RADIUS..=CHUNK_LOAD_RADIUS {
            coords.insert(ChunkCoord {
                x: center.x + x_offset,
                z: center.z + z_offset,
            });
        }
    }
    coords
}

pub(crate) fn definition_chunk_coord(world_coord: ChunkCoord) -> ChunkCoord {
    // ワールドチャンク座標を、定義チャンク座標にラップする
    ChunkCoord {
        x: world_coord.x.rem_euclid(WORLD_CHUNKS_PER_AXIS),
        z: world_coord.z.rem_euclid(WORLD_CHUNKS_PER_AXIS),
    }
}


fn spawn_chunk(// チャンク定義から描画用の親Entityと各配置Entityを生成する
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    assets: &WorldRenderAssets,
    world_map: &WorldMap,
    world_coord: ChunkCoord,
    definition: &ChunkDefinition,
    flags: &WorldFlags,
    progress: &WorldProgress,
) {
    let chunk_root = commands
        .spawn((
            ChunkEntity(world_coord),
            Transform::IDENTITY,
            Visibility::Inherited,
        ))
        .id();
    let center = world_coord.center();

    let terrain_mesh = if definition.terrain == TerrainKind::GentleSlope {
        meshes.add(create_slope_mesh(world_map, world_coord))
    } else {
        assets.ground_mesh.clone()
    };
    commands.spawn((
        ChildOf(chunk_root),
        Mesh3d(terrain_mesh),
        MeshMaterial3d(assets.ground_material.clone()),
        Transform::from_translation(center),
    ));

    if definition.terrain == TerrainKind::RiverChannel {
        commands.spawn((
            ChildOf(chunk_root),
            RiverChannel,
            Mesh3d(assets.river_mesh.clone()),
            MeshMaterial3d(assets.river_material.clone()),
            Transform::from_xyz(
                center.x + RIVER_CHANNEL_CENTER_X_OFFSET,
                RIVER_CHANNEL_SURFACE_THICKNESS / 2.0,
                center.z,
            ),
        ));
    }

    for placement in &definition.objects {
        if progress.collected_objects.contains(&placement.id)
            || progress.chopped_trees.contains(&placement.id)
            || placement
                .completion_flag
                .is_some_and(|flag| flags.is_set(flag))
        {
            continue;
        }
        let mut world_position = world_coord.world_position(placement.local_position);
        world_position.y += world_map.ground_height(world_position.x, world_position.z);
        spawn_object(
            commands,
            assets,
            chunk_root,
            world_position,
            *placement,
            flags,
        );
    }
}

fn create_slope_mesh(world_map: &WorldMap, chunk: ChunkCoord) -> Mesh {
    let row_length = TERRAIN_MESH_SUBDIVISIONS + 1;
    let vertex_count = row_length * row_length;
    let center = chunk.center();
    let step = CHUNK_SIZE / TERRAIN_MESH_SUBDIVISIONS as f32;
    let mut positions = Vec::with_capacity(vertex_count);
    let mut normals = Vec::with_capacity(vertex_count);

    for z_index in 0..=TERRAIN_MESH_SUBDIVISIONS {
        let local_z = -CHUNK_SIZE / 2.0 + z_index as f32 * step;
        for x_index in 0..=TERRAIN_MESH_SUBDIVISIONS {
            let local_x = -CHUNK_SIZE / 2.0 + x_index as f32 * step;
            let world_x = center.x + local_x;
            let world_z = center.z + local_z;
            let height = world_map.ground_height(world_x, world_z);
            let normal_delta = 0.05;
            let slope_x = (world_map.ground_height(world_x + normal_delta, world_z)
                - world_map.ground_height(world_x - normal_delta, world_z))
                / (normal_delta * 2.0);
            let slope_z = (world_map.ground_height(world_x, world_z + normal_delta)
                - world_map.ground_height(world_x, world_z - normal_delta))
                / (normal_delta * 2.0);
            let normal = Vec3::new(-slope_x, 1.0, -slope_z).normalize();

            positions.push([local_x, height, local_z]);
            normals.push(normal.to_array());
        }
    }

    let mut indices = Vec::with_capacity(TERRAIN_MESH_SUBDIVISIONS.pow(2) * 6);
    for z_index in 0..TERRAIN_MESH_SUBDIVISIONS {
        for x_index in 0..TERRAIN_MESH_SUBDIVISIONS {
            let top_left = (z_index * row_length + x_index) as u32;
            let top_right = top_left + 1;
            let bottom_left = top_left + row_length as u32;
            let bottom_right = bottom_left + 1;
            indices.extend_from_slice(&[
                top_left,
                bottom_left,
                top_right,
                top_right,
                bottom_left,
                bottom_right,
            ]);
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

/// 配置定義から仮メッシュとColliderデータを持つEntityを生成する。
fn spawn_object(
    commands: &mut Commands,
    assets: &WorldRenderAssets,
    chunk_root: Entity,
    world_position: Vec3,
    placement: ObjectPlacement,
    flags: &WorldFlags,
) {
    if placement.kind == ObjectKind::Tree {
        let tree = commands
            .spawn((
                ChildOf(chunk_root),
                ObjectEntity(placement.id),
                ObjectCollider(placement.collider),
                Interactable {
                    action: InteractionAction::ChopTree,
                },
                WorldAssetRoot(assets.tree_scene.clone()),
                Transform::from_translation(world_position).with_rotation(placement.rotation),
                Visibility::Inherited,
            ))
            .id();
        spawn_interaction_popup(commands, assets, tree, 3.2);
        return;
    }

    if placement.kind == ObjectKind::AxePickup {
        // 斧の配置は拾うと消えるのでInteractionPopupを生成する
        let axe = commands
            .spawn((
                ChildOf(chunk_root),
                ObjectEntity(placement.id),
                ObjectCollider(placement.collider),
                Interactable {
                    action: InteractionAction::PickUp(ItemKind::Axe),
                },
                WorldAssetRoot(assets.axe_scene.clone()),
                Transform::from_translation(world_position),
                Visibility::Inherited,
            ))
            .id();
        spawn_interaction_popup(commands, assets, axe, 0.8);
        return;
    }

    let model = match placement.kind {// 配置の種類に応じて描画用のモデルを選択する
        ObjectKind::Log => assets.log_scene.clone(),
        ObjectKind::BridgeDeck => assets.bridge_deck_scene.clone(),
        ObjectKind::BridgeRail => assets.bridge_rail_scene.clone(),
        ObjectKind::Tree | ObjectKind::AxePickup => unreachable!(),
    };
    let transform = Transform {// 配置の位置と回転を設定する
        translation: world_position,
        rotation: placement.rotation,
        ..default()
    };
    let components = (
        // 配置のEntityに付与するコンポーネントをまとめる
        ObjectEntity(placement.id),
        ObjectCollider(placement.collider),
        WorldAssetRoot(model),
        transform,
    );

    if placement.kind == ObjectKind::Log {
        // 丸太の配置は橋を作るための対話アクションを持つのでInteractionPopupを生成する
        let target = commands
            .spawn((
                ChildOf(chunk_root),
                components,
                Interactable {
                    action: InteractionAction::BuildBridge(
                        placement
                            .completion_flag
                            .expect("bridge site must have a completion flag"),
                    ),
                },
                Visibility::Inherited,
            ))
            .id();
        commands.spawn((
            InteractionPopup,
            ChildOf(target),
            Mesh3d(assets.popup_mesh.clone()),
            MeshMaterial3d(assets.object_material.clone()),
            Transform::from_xyz(0.0, 1.2, 0.0),
            Visibility::Hidden,
        ));
    } else {
        commands.spawn((
            ChildOf(chunk_root),
            BridgePart,
            if flags.is_set(WorldFlag::BridgeBuilt) {
                Visibility::Visible
            } else {
                Visibility::Hidden
            },
            components,
        ));
    }
}

fn spawn_interaction_popup(// 配置のEntityの上にInteractionPopupを生成する
    commands: &mut Commands,
    assets: &WorldRenderAssets,
    target: Entity,
    height: f32,
) {
    commands.spawn((
        InteractionPopup,
        ChildOf(target),
        Mesh3d(assets.popup_mesh.clone()),
        MeshMaterial3d(assets.object_material.clone()),
        Transform::from_xyz(0.0, height, 0.0),
        Visibility::Hidden,
    ));
}

pub(crate) fn react_to_flag_changes(// WorldFlagの変更に応じて、橋のパーツの表示状態を切り替える
    mut flag_changed: MessageReader<FlagChanged>,
    mut bridge_parts: Query<&mut Visibility, With<BridgePart>>,
) {
    for change in flag_changed.read() {
        if change.flag == WorldFlag::BridgeBuilt && change.value {
            for mut visibility in &mut bridge_parts {
                *visibility = Visibility::Visible;
            }
        }
    }
}

#[cfg(test)]
mod tests {// テスト用のモジュール
    use super::*;
    use crate::interaction::perform_interaction;
    use crate::inventory::{Inventory, ItemKind, select_inventory_item};
    use crate::player::PLAYER_GROUND_Y;
    use crate::world::{ObjectInstanceId, WorldMap};

    #[test]
    fn npc_dialogue_visibility_uses_horizontal_distance() {
        // NPCの会話HUDの表示判定が水平距離を使用していることを確認する
        let npc_position = Vec3::new(NPC_START_X, 0.0, 0.0);

        assert!(is_near_npc(Vec3::new(0.0, 0.6, 0.0), npc_position));
        assert!(is_near_npc(
            npc_position + Vec3::X * NPC_DIALOGUE_DISTANCE,
            npc_position,
        ));
        assert!(!is_near_npc(
            npc_position + Vec3::X * (NPC_DIALOGUE_DISTANCE + 0.01),
            npc_position,
        ));
        assert!(is_near_npc(npc_position + Vec3::Y * 100.0, npc_position,));
    }

    #[test]
    fn riverside_npc_changes_line_when_the_bridge_is_built() {
        // 橋が完成したときにNPCの台詞が変わることを確認する
        assert_eq!(riverside_npc_line(false), "かわを わたりたいな");
        assert_eq!(riverside_npc_line(true), "ありがとう！");
    }

    #[test]
    fn slope_mesh_has_subdivided_triangles_for_smooth_height_changes() {
        let world_map = WorldMap::prototype();
        let mesh = create_slope_mesh(&world_map, ChunkCoord { x: 7, z: 10 });

        assert_eq!(
            mesh.count_vertices(),
            (TERRAIN_MESH_SUBDIVISIONS + 1).pow(2)
        );
        assert_eq!(
            mesh.indices().expect("slope mesh has triangle indices").len(),
            TERRAIN_MESH_SUBDIVISIONS.pow(2) * 6
        );
    }

    #[test]
    fn prototype_progression_survives_chunk_unload_and_reload() {
        let render_assets = WorldRenderAssets {
            ground_mesh: Handle::default(),
            river_mesh: Handle::default(),
            popup_mesh: Handle::default(),
            tree_scene: Handle::default(),
            axe_scene: Handle::default(),
            log_scene: Handle::default(),
            bridge_deck_scene: Handle::default(),
            bridge_rail_scene: Handle::default(),
            player_scene: Handle::default(),
            npc_scene: Handle::default(),
            ground_material: Handle::default(),
            river_material: Handle::default(),
            object_material: Handle::default(),
        };
        let mut app = App::new();
        app.insert_resource(WorldMap::prototype())
            .insert_resource(render_assets)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<WorldFlags>()
            .init_resource::<WorldProgress>()
            .init_resource::<Inventory>()
            .init_resource::<ButtonInput<KeyCode>>()
            .add_message::<FlagChanged>()
            .add_systems(
                Update,
                (
                    sync_loaded_chunks,
                    select_inventory_item,
                    perform_interaction,
                    react_to_flag_changes,
                )
                    .chain(),
            );

        let player = app
            .world_mut()
            .spawn((
                Player::default(),
                Transform::from_xyz(1.0, PLAYER_GROUND_Y, 1.0),
            ))
            .id();
        app.update();

        press_key_and_update(&mut app, KeyCode::KeyE);
        assert_eq!(
            app.world().resource::<Inventory>().count(ItemKind::Axe),
            1
        );
        assert!(
            app.world()
                .resource::<WorldProgress>()
                .collected_objects
                .contains(&ObjectInstanceId(500))
        );

        app.world_mut()
            .get_mut::<Transform>(player)
            .expect("test player still exists")
            .translation = Vec3::new(-2.0, PLAYER_GROUND_Y, 0.0);
        assert!(
            app.world_mut()
                .query::<&ObjectEntity>()
                .iter(app.world())
                .any(|object| object.0 == ObjectInstanceId(501))
        );
        press_key_and_update(&mut app, KeyCode::KeyE);
        assert_eq!(
            app.world().resource::<Inventory>().count(ItemKind::Wood),
            1
        );
        assert!(
            app.world()
                .resource::<WorldProgress>()
                .chopped_trees
                .contains(&ObjectInstanceId(501))
        );

        press_key_and_update(&mut app, KeyCode::Digit2);
        assert!(
            app.world()
                .resource::<Inventory>()
                .is_selected(ItemKind::Wood)
        );
        app.world_mut()
            .get_mut::<Transform>(player)
            .expect("test player still exists")
            .translation = Vec3::new(3.0, PLAYER_GROUND_Y, 0.0);
        press_key_and_update(&mut app, KeyCode::KeyE);
        assert!(app.world().resource::<WorldFlags>().is_set(WorldFlag::BridgeBuilt));
        assert_eq!(
            app.world().resource::<Inventory>().count(ItemKind::Wood),
            0
        );

        app.world_mut()
            .get_mut::<Transform>(player)
            .expect("test player still exists")
            .translation = Vec3::new(45.0, PLAYER_GROUND_Y, 0.0);
        app.update();
        app.world_mut()
            .get_mut::<Transform>(player)
            .expect("test player still exists")
            .translation = Vec3::new(0.0, PLAYER_GROUND_Y, 0.0);
        app.update();

        let mut object_entities = app.world_mut().query::<&ObjectEntity>();
        let reloaded_object_ids: Vec<_> = object_entities
            .iter(app.world())
            .map(|object| object.0)
            .collect();
        assert!(!reloaded_object_ids.contains(&ObjectInstanceId(500)));
        assert!(!reloaded_object_ids.contains(&ObjectInstanceId(501)));
        assert!(!reloaded_object_ids.contains(&ObjectInstanceId(1)));

        let mut bridge_parts = app
            .world_mut()
            .query_filtered::<&Visibility, With<BridgePart>>();
        let reloaded_bridge_parts: Vec<_> = bridge_parts.iter(app.world()).collect();
        assert_eq!(reloaded_bridge_parts.len(), 3);
        assert!(
            reloaded_bridge_parts
                .iter()
                .all(|visibility| **visibility == Visibility::Visible)
        );

        assert!(!app.world().resource::<WorldMap>().blocks_movement(
            Vec3::new(5.8, 0.0, 0.0),
            app.world().resource::<WorldFlags>().is_set(WorldFlag::BridgeBuilt),
            0.4,
        ));
    }

    fn press_key_and_update(app: &mut App, key: KeyCode) {
        let mut keyboard = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keyboard.reset_all();
        keyboard.press(key);
        drop(keyboard);
        app.update();
    }
}
