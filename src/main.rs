//! ノスタルジック・ワーク・エクスプローラーの初期プロトタイプ。
//main.rs
mod interaction;
mod inventory;
mod player;
mod quest;
mod save;
mod scene;
mod world;

use bevy::prelude::*;
use bevy::app::OnAppExitSystems;
use interaction::{perform_interaction, update_interaction_popups, update_interaction_prompt};
use player::{move_player, orbit_camera};
use world::WorldMap;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum UpdateSet {
    NotebookInput,
    Player,
    InventorySelection,
    ChunkStreaming,
    InteractionActions,
    WorldFeedback,
    Npc,
    QuestProgress,
    Hud,
}

fn main() {
    let world_map = WorldMap::prototype();
    if let Err(errors) = world_map.validate() {
        let details = errors
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        panic!("ワールドマップ定義が不正です:\n{details}");
    }
    if let Err(errors) = quest::validate_definitions(&world_map) {
        panic!("クエスト定義が不正です:\n{}", errors.join("\n"));
    }
    let save_file = save::default_save_path()
        .unwrap_or_else(|error| panic!("セーブデータ保存先を決定できませんでした: {error}"));
    let loaded_game = save::load_game(&world_map, &save_file)
        .unwrap_or_else(|error| panic!("セーブデータを読み込めませんでした: {error}"));

    App::new()
        .add_plugins(DefaultPlugins)
        .insert_resource(world_map)
        .insert_resource(save_file)
        .insert_resource(loaded_game.flags)
        .insert_resource(loaded_game.progress)
        .insert_resource(loaded_game.inventory)
        .insert_resource(loaded_game.quests)
        .init_resource::<player::CameraOrbit>()
        .init_resource::<quest::NotebookState>()
        .add_message::<scene::FlagChanged>()
        .add_systems(
            Startup,
            (
                scene::setup_scene,
                quest::setup_notebook,
                scene::validate_required_entities,
            )
                .chain(),
        )
        .add_systems(Last, save::save_on_exit.in_set(OnAppExitSystems))
        .add_systems(Update, scene::log_asset_load_failures)
        .configure_sets(
            Update,
            (
                UpdateSet::NotebookInput,
                UpdateSet::Player,
                UpdateSet::InventorySelection,
                UpdateSet::ChunkStreaming,
                UpdateSet::InteractionActions,
                UpdateSet::WorldFeedback,
                UpdateSet::Npc,
                UpdateSet::QuestProgress,
                UpdateSet::Hud,
            )
                .chain(),
        )
        .add_systems(
            Update,
            quest::toggle_notebook.in_set(UpdateSet::NotebookInput),
        )
        .add_systems(
            Update,
            (orbit_camera, move_player)
                .chain()
                .run_if(quest::gameplay_enabled)
                .in_set(UpdateSet::Player),
        )
        .add_systems(
            Update,
            inventory::select_inventory_item
                .run_if(quest::gameplay_enabled)
                .in_set(UpdateSet::InventorySelection),
        )
        .add_systems(
            Update,
            scene::sync_loaded_chunks.in_set(UpdateSet::ChunkStreaming),
        )
        .add_systems(
            Update,
            (
                perform_interaction.run_if(quest::gameplay_enabled),
                update_interaction_prompt,
                update_interaction_popups,
            )
                .chain()
                .in_set(UpdateSet::InteractionActions),
        )
        .add_systems(
            Update,
            scene::react_to_flag_changes.in_set(UpdateSet::WorldFeedback),
        )
        .add_systems(Update, scene::update_riverside_npc.in_set(UpdateSet::Npc))
        .add_systems(
            Update,
            quest::update_quests.in_set(UpdateSet::QuestProgress),
        )
        .add_systems(
            Update,
            (inventory::update_inventory_hud, quest::update_notebook_ui)
                .in_set(UpdateSet::Hud),
        )
        .run();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interaction::can_perform_action;
    use crate::inventory::{Inventory, ItemKind};
    use crate::player::wrap_world_coordinate;
    use crate::scene::{
        definition_chunk_coord, nearby_chunk_coords, InteractionAction, WorldFlags, WorldProgress,
    };
    use crate::world::{ChunkCoord, ObjectInstanceId, WorldFlag, WORLD_HALF_SIZE};

    #[test]
    fn field_edges_wrap_to_the_opposite_side() {// ワールドの座標がワールドの境界を超えた場合に反対側の座標にラップされることを確認する
        assert_eq!(
            wrap_world_coordinate(WORLD_HALF_SIZE + 1.0),
            -WORLD_HALF_SIZE + 1.0
        );
        assert_eq!(
            wrap_world_coordinate(-WORLD_HALF_SIZE - 1.0),
            WORLD_HALF_SIZE - 1.0
        );
    }

    #[test]
    fn chunk_streaming_keeps_a_three_by_three_neighborhood() {// チャンクストリーミングが3x3の近傍を維持することを確認する
        let center = ChunkCoord { x: 10, z: 10 };
        let nearby = nearby_chunk_coords(center);

        assert_eq!(nearby.len(), 9);
        assert!(nearby.contains(&center));
        assert!(nearby.contains(&ChunkCoord { x: 9, z: 9 }));
        assert!(nearby.contains(&ChunkCoord { x: 11, z: 11 }));
    }

    #[test]
    fn streamed_world_chunks_map_to_wrapped_definitions() {// ストリーミングされたワールドチャンクがラップされた定義にマップされることを確認する
        assert_eq!(
            definition_chunk_coord(ChunkCoord { x: -1, z: 20 }),
            ChunkCoord { x: 19, z: 0 }
        );
    }

    #[test]
    fn bridge_flag_survives_chunk_unload_and_controls_the_reloaded_scene() {// ブリッジフラグがチャンクのアンロード後も保持され、再ロードされたシーンを制御することを確認する
        let mut flags = WorldFlags::default();
        let world_map = WorldMap::prototype();
        let river_position = Vec3::new(5.8, 0.0, 0.0);

        assert!(world_map.blocks_movement(
            river_position,
            flags.is_set(WorldFlag::BridgeBuilt),
            0.4
        ));

        // チャンクEntityの破棄ではResourceの進行状態を変更しない。
        flags.set(WorldFlag::BridgeBuilt, true);
        let bridge_chunk_reloaded = flags.is_set(WorldFlag::BridgeBuilt);

        assert!(bridge_chunk_reloaded);
        assert!(!world_map.blocks_movement(river_position, bridge_chunk_reloaded, 0.4));
    }

    #[test]
    fn forward_input_follows_player_rotation() {// プレイヤーの回転に従って前方入力が変化することを確認する
        let forward_input = Vec3::new(0.0, 0.0, -1.0);
        let facing_south = Quat::IDENTITY * forward_input;
        let facing_west = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2) * forward_input;

        assert!(facing_south.z < -0.99);
        assert!(facing_west.x < -0.99);
    }

    #[test]
    fn inventory_items_can_be_selected_and_consumed() {// 在庫アイテムが選択され、消費されることを確認する
        let mut inventory = Inventory::default();
        inventory.add(ItemKind::Axe, 1);
        inventory.add(ItemKind::Wood, 2);

        assert_eq!(inventory.selected, Some(ItemKind::Axe.id()));
        inventory.select(ItemKind::Wood);
        assert_eq!(inventory.selected, Some(ItemKind::Wood.id()));
        assert!(inventory.consume(ItemKind::Wood, 1));
        assert_eq!(inventory.count(ItemKind::Wood), 1);
        assert!(!inventory.consume(ItemKind::Wood, 2));
        assert!(inventory.consume(ItemKind::Wood, 1));
        assert!(!inventory.items.contains_key(&ItemKind::Wood.id()));
        assert_eq!(inventory.selected, Some(ItemKind::Axe.id()));
    }

    #[test]
    fn interaction_actions_require_the_selected_item_and_only_complete_once() {// 対話アクションが選択されたアイテムを必要とし、一度だけ完了することを確認する
        let mut inventory = Inventory::default();
        let mut flags = WorldFlags::default();
        let mut progress = WorldProgress::default();
        let tree_id = ObjectInstanceId(501);

        assert!(!can_perform_action(
            InteractionAction::ChopTree,
            tree_id,
            &inventory,
            &flags,
            &progress,
        ));
        inventory.add(ItemKind::Axe, 1);
        assert!(can_perform_action(
            InteractionAction::ChopTree,
            tree_id,
            &inventory,
            &flags,
            &progress,
        ));
        progress.chopped_trees.insert(tree_id);
        assert!(!can_perform_action(
            InteractionAction::ChopTree,
            tree_id,
            &inventory,
            &flags,
            &progress,
        ));

        inventory.add(ItemKind::Wood, 1);
        inventory.select(ItemKind::Wood);
        let bridge_action = InteractionAction::BuildBridge(WorldFlag::BridgeBuilt);
        assert!(can_perform_action(
            bridge_action,
            ObjectInstanceId(1),
            &inventory,
            &flags,
            &progress,
        ));
        flags.set(WorldFlag::BridgeBuilt, true);
        assert!(!can_perform_action(
            bridge_action,
            ObjectInstanceId(1),
            &inventory,
            &flags,
            &progress,
        ));
    }
}
