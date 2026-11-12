use std::collections::{HashMap, HashSet};
use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::inventory::{Inventory, ItemId, ItemKind};
use crate::quest::{self, BRIDGE_QUEST_ID, QuestId, QuestLog};
use crate::scene::{WorldFlags, WorldProgress};
use crate::world::{ObjectInstanceId, ObjectKind, WorldFlag, WorldMap};

const SAVE_VERSION: u32 = 2;
const LEGACY_SAVE_VERSION: u32 = 1;
const SAVE_FILE_NAME: &str = "progress.ron";

#[derive(Serialize, Deserialize)]
struct SaveData {
    version: u32,
    items: Vec<SavedItem>,
    selected_item_id: Option<u16>,
    bridge_built: bool,
    collected_object_ids: Vec<u32>,
    chopped_tree_ids: Vec<u32>,
    #[serde(
        default,
        deserialize_with = "deserialize_quest_ids",
        serialize_with = "serialize_quest_ids"
    )]
    started_quest_ids: Option<Vec<u16>>,
}

fn deserialize_quest_ids<'de, D>(deserializer: D) -> Result<Option<Vec<u16>>, D::Error>
where
    D: Deserializer<'de>,
{
    Vec::<u16>::deserialize(deserializer).map(Some)
}

fn serialize_quest_ids<S>(ids: &Option<Vec<u16>>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    ids.as_ref()
        .ok_or_else(|| serde::ser::Error::custom("開始済みクエストIDがありません"))?
        .serialize(serializer)
}

#[derive(Serialize, Deserialize)]
struct SavedItem {
    id: u16,
    count: u32,
}

pub(crate) struct LoadedGame {
    pub(crate) inventory: Inventory,
    pub(crate) flags: WorldFlags,
    pub(crate) progress: WorldProgress,
    pub(crate) quests: QuestLog,
}

#[derive(Resource)]
pub(crate) struct SaveFilePath(pub(crate) PathBuf);

pub(crate) fn default_save_path() -> Result<SaveFilePath, String> {
    resolve_save_path().map(SaveFilePath)
}

pub(crate) fn load_game(
    world_map: &WorldMap,
    save_file: &SaveFilePath,
) -> Result<LoadedGame, String> {
    let path = &save_file.0;
    let contents = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(default_game());
        }
        Err(error) => {
            return Err(format!("{} を読み込めません: {error}", path.display()));
        }
    };

    let data: SaveData = ron::from_str(&contents)
        .map_err(|error| format!("{} の形式が不正です: {error}", path.display()))?;
    game_from_save_data(data, world_map)
        .map_err(|error| format!("{} の内容が不正です: {error}", path.display()))
}

pub(crate) fn save_on_exit(
    mut exits: MessageReader<AppExit>,
    save_file: Res<SaveFilePath>,
    inventory: Res<Inventory>,
    flags: Res<WorldFlags>,
    progress: Res<WorldProgress>,
    quests: Res<QuestLog>,
) {
    if exits.read().next().is_none() {
        return;
    }

    if let Err(error) = save_game_to_path(&save_file.0, &inventory, &flags, &progress, &quests) {
        error!("セーブデータを保存できませんでした: {error}");
    }
}

fn save_game_to_path(
    path: &Path,
    inventory: &Inventory,
    flags: &WorldFlags,
    progress: &WorldProgress,
    quests: &QuestLog,
) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("保存先ディレクトリがありません: {}", path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("{} を作成できません: {error}", parent.display()))?;

    let data = save_data_from_game(inventory, flags, progress, quests);
    let contents =
        ron::to_string(&data).map_err(|error| format!("保存データを作れません: {error}"))?;
    let temporary_path = path.with_extension("ron.tmp");
    let write_result = write_temporary_file(&temporary_path, contents.as_bytes())
        .and_then(|()| fs::rename(&temporary_path, &path));
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temporary_path);
        return Err(format!("{} に書き込めません: {error}", path.display()));
    }
    Ok(())
}

fn write_temporary_file(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let mut file = File::create(path)?;
    file.write_all(contents)?;
    file.sync_all()
}

fn resolve_save_path() -> Result<PathBuf, String> {
    if let Some(data_dir) = env::var_os("XDG_DATA_HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(data_dir)
            .join("hako_work")
            .join(SAVE_FILE_NAME));
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(data_dir) = env::var_os("APPDATA").filter(|value| !value.is_empty()) {
            return Ok(PathBuf::from(data_dir)
                .join("hako_work")
                .join(SAVE_FILE_NAME));
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(home_dir) = env::var_os("HOME").filter(|value| !value.is_empty()) {
            return Ok(PathBuf::from(home_dir)
                .join("Library")
                .join("Application Support")
                .join("hako_work")
                .join(SAVE_FILE_NAME));
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Some(home_dir) = env::var_os("HOME").filter(|value| !value.is_empty()) {
            return Ok(PathBuf::from(home_dir)
                .join(".local")
                .join("share")
                .join("hako_work")
                .join(SAVE_FILE_NAME));
        }
    }

    Err("ユーザーデータ保存先を決定できません".to_owned())
}

fn default_game() -> LoadedGame {
    LoadedGame {
        inventory: Inventory::default(),
        flags: WorldFlags::default(),
        progress: WorldProgress::default(),
        quests: QuestLog::default(),
    }
}

fn save_data_from_game(
    inventory: &Inventory,
    flags: &WorldFlags,
    progress: &WorldProgress,
    quests: &QuestLog,
) -> SaveData {
    let mut started_quest_ids: Vec<_> = quests.started_ids().map(|id| id.0).collect();
    started_quest_ids.sort_unstable();
    SaveData {
        version: SAVE_VERSION,
        items: inventory
            .items
            .iter()
            .filter(|(_, count)| **count > 0)
            .map(|(id, count)| SavedItem {
                id: id.0,
                count: *count,
            })
            .collect(),
        selected_item_id: inventory.selected.map(|id| id.0),
        bridge_built: flags.is_set(WorldFlag::BridgeBuilt),
        collected_object_ids: progress.collected_objects.iter().map(|id| id.0).collect(),
        chopped_tree_ids: progress.chopped_trees.iter().map(|id| id.0).collect(),
        started_quest_ids: Some(started_quest_ids),
    }
}

fn game_from_save_data(data: SaveData, world_map: &WorldMap) -> Result<LoadedGame, String> {
    if data.version != LEGACY_SAVE_VERSION && data.version != SAVE_VERSION {
        return Err(format!(
            "未対応の保存形式バージョンです: {} (対応: {LEGACY_SAVE_VERSION}, {SAVE_VERSION})",
            data.version,
        ));
    }

    let mut items = HashMap::new();
    let mut seen_item_ids = HashSet::new();
    let mut empty_item_ids = HashSet::new();
    for saved_item in data.items {
        let id = ItemId(saved_item.id);
        if ItemKind::from_id(id).is_none() {
            return Err(format!("未知のアイテムIDです: {}", saved_item.id));
        }
        if !seen_item_ids.insert(id) {
            return Err(format!("アイテムIDが重複しています: {}", saved_item.id));
        }
        if saved_item.count == 0 {
            empty_item_ids.insert(id);
        } else {
            items.insert(id, saved_item.count);
        }
    }

    let selected = data
        .selected_item_id
        .map(ItemId)
        .map(|id| {
            if ItemKind::from_id(id).is_none() {
                return Err(format!("選択中のアイテムIDが不正です: {}", id.0));
            }
            if items.contains_key(&id) {
                Ok(Some(id))
            } else if empty_item_ids.contains(&id) {
                Ok(None)
            } else {
                Err(format!("選択中のアイテムを所持していません: {}", id.0))
            }
        })
        .transpose()?
        .flatten();

    let object_kinds: HashMap<_, _> = world_map
        .chunks
        .values()
        .flat_map(|chunk| chunk.objects.iter().map(|object| (object.id, object.kind)))
        .collect();
    let collected_objects = checked_object_ids(
        &data.collected_object_ids,
        "取得済み",
        &object_kinds,
        ObjectKind::AxePickup,
    )?;
    let chopped_trees = checked_object_ids(
        &data.chopped_tree_ids,
        "伐採済み",
        &object_kinds,
        ObjectKind::Tree,
    )?;

    let mut inventory = Inventory {
        items,
        selected: None,
    };
    if let Some(id) = selected {
        inventory.selected = Some(id);
    }

    let mut flags = WorldFlags::default();
    flags.set(WorldFlag::BridgeBuilt, data.bridge_built);

    let started_quest_ids = match data.started_quest_ids {
        Some(ids) => ids,
        None if data.version == LEGACY_SAVE_VERSION => Vec::new(),
        None => return Err("保存データに開始済みクエストIDがありません".to_owned()),
    };
    let mut quests = QuestLog::default();
    for raw_id in started_quest_ids {
        let id = QuestId(raw_id);
        if quest::definition(id).is_none() {
            return Err(format!("未知のクエストIDです: {raw_id}"));
        }
        if !quests.start(id) {
            return Err(format!("クエストIDが重複しています: {raw_id}"));
        }
    }
    if data.version == LEGACY_SAVE_VERSION && data.bridge_built {
        quests.start(BRIDGE_QUEST_ID);
    }

    Ok(LoadedGame {
        inventory,
        flags,
        progress: WorldProgress {
            collected_objects,
            chopped_trees,
        },
        quests,
    })
}

fn checked_object_ids(
    ids: &[u32],
    label: &str,
    object_kinds: &HashMap<ObjectInstanceId, ObjectKind>,
    expected_kind: ObjectKind,
) -> Result<HashSet<ObjectInstanceId>, String> {
    let mut checked_ids = HashSet::with_capacity(ids.len());
    for raw_id in ids {
        let id = ObjectInstanceId(*raw_id);
        if !checked_ids.insert(id) {
            return Err(format!("{label}オブジェクトIDが重複しています: {}", id.0));
        }
        if object_kinds.get(&id) != Some(&expected_kind) {
            return Err(format!(
                "{label}オブジェクトIDがワールド定義にありません: {}",
                id.0
            ));
        }
    }
    Ok(checked_ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::app::OnAppExitSystems;

    #[test]
    fn save_data_round_trips_progress_and_inventory() {
        let mut inventory = Inventory::default();
        inventory.add(ItemKind::Axe, 1);
        inventory.add(ItemKind::Wood, 2);
        inventory.select(ItemKind::Wood);
        let mut flags = WorldFlags::default();
        flags.set(WorldFlag::BridgeBuilt, true);
        let progress = WorldProgress {
            collected_objects: HashSet::from([ObjectInstanceId(500)]),
            chopped_trees: HashSet::from([ObjectInstanceId(501)]),
        };
        let mut quests = QuestLog::default();
        quests.start(BRIDGE_QUEST_ID);
        let data = save_data_from_game(&inventory, &flags, &progress, &quests);
        let encoded = ron::to_string(&data).expect("save data serializes");
        assert!(encoded.contains("started_quest_ids:[1]"));
        let decoded = ron::from_str(&encoded).expect("save data deserializes");
        let loaded = game_from_save_data(decoded, &WorldMap::prototype())
            .expect("valid saved progress loads");

        assert_eq!(loaded.inventory.count(ItemKind::Axe), 1);
        assert_eq!(loaded.inventory.count(ItemKind::Wood), 2);
        assert!(loaded.inventory.is_selected(ItemKind::Wood));
        assert!(loaded.flags.is_set(WorldFlag::BridgeBuilt));
        assert!(loaded.quests.has_started(BRIDGE_QUEST_ID));
        assert!(loaded
            .progress
            .collected_objects
            .contains(&ObjectInstanceId(500)));
        assert!(loaded
            .progress
            .chopped_trees
            .contains(&ObjectInstanceId(501)));
    }

    #[test]
    fn invalid_saved_item_and_object_ids_are_rejected() {
        let world_map = WorldMap::prototype();
        let mut data = save_data_from_game(
            &Inventory::default(),
            &WorldFlags::default(),
            &WorldProgress::default(),
            &QuestLog::default(),
        );
        data.items.push(SavedItem { id: 999, count: 1 });
        assert!(game_from_save_data(data, &world_map).is_err());

        let mut data = save_data_from_game(
            &Inventory::default(),
            &WorldFlags::default(),
            &WorldProgress::default(),
            &QuestLog::default(),
        );
        data.chopped_tree_ids.push(500);
        assert!(game_from_save_data(data, &world_map).is_err());
    }

    #[test]
    fn previously_generated_tree_progress_remains_valid_on_the_hill() {
        let world_map = WorldMap::prototype();
        let mut data = save_data_from_game(
            &Inventory::default(),
            &WorldFlags::default(),
            &WorldProgress::default(),
            &QuestLog::default(),
        );
        data.chopped_tree_ids.push(11_606);

        let loaded = game_from_save_data(data, &world_map)
            .expect("tree IDs from before the hill was added remain valid");
        assert!(loaded
            .progress
            .chopped_trees
            .contains(&ObjectInstanceId(11_606)));
    }

    #[test]
    fn legacy_zero_count_items_are_ignored_and_not_written_back() {
        let world_map = WorldMap::prototype();
        let mut data = save_data_from_game(
            &Inventory::default(),
            &WorldFlags::default(),
            &WorldProgress::default(),
            &QuestLog::default(),
        );
        data.items.push(SavedItem { id: 2, count: 0 });
        data.selected_item_id = Some(2);

        let loaded =
            game_from_save_data(data, &world_map).expect("legacy zero-count item is normalized");
        assert_eq!(loaded.inventory.count(ItemKind::Wood), 0);
        assert_eq!(loaded.inventory.selected, None);

        let data = save_data_from_game(
            &loaded.inventory,
            &loaded.flags,
            &loaded.progress,
            &loaded.quests,
        );
        assert!(data.items.is_empty());
        assert_eq!(data.selected_item_id, None);
    }

    #[test]
    fn unsupported_save_versions_are_rejected() {
        let mut data = save_data_from_game(
            &Inventory::default(),
            &WorldFlags::default(),
            &WorldProgress::default(),
            &QuestLog::default(),
        );
        data.version += 1;
        assert!(game_from_save_data(data, &WorldMap::prototype()).is_err());
    }

    #[test]
    fn legacy_completed_bridge_migrates_to_a_completed_quest() {
        let contents = "(version:1,items:[(id:1,count:1)],selected_item_id:Some(1),\
            bridge_built:true,collected_object_ids:[500],chopped_tree_ids:[501])";
        let data: SaveData = ron::from_str(contents).expect("version 1 save is readable");
        let loaded = game_from_save_data(data, &WorldMap::prototype())
            .expect("version 1 progress is migrated");
        assert_eq!(loaded.inventory.count(ItemKind::Axe), 1);
        assert!(loaded.flags.is_set(WorldFlag::BridgeBuilt));
        assert!(loaded.quests.has_started(BRIDGE_QUEST_ID));
        assert!(loaded.progress.chopped_trees.contains(&ObjectInstanceId(501)));

        let saved = save_data_from_game(
            &loaded.inventory,
            &loaded.flags,
            &loaded.progress,
            &loaded.quests,
        );
        assert_eq!(saved.version, SAVE_VERSION);
        assert_eq!(saved.started_quest_ids, Some(vec![BRIDGE_QUEST_ID.0]));
    }

    #[test]
    fn legacy_unfinished_game_keeps_its_materials() {
        let contents = "(version:1,items:[(id:2,count:1)],selected_item_id:Some(2),\
            bridge_built:false,collected_object_ids:[],chopped_tree_ids:[501])";
        let data: SaveData = ron::from_str(contents).expect("version 1 save is readable");
        let loaded = game_from_save_data(data, &WorldMap::prototype())
            .expect("unfinished version 1 progress is migrated");
        assert_eq!(loaded.inventory.count(ItemKind::Wood), 1);
        assert!(loaded.inventory.is_selected(ItemKind::Wood));
        assert!(!loaded.flags.is_set(WorldFlag::BridgeBuilt));
        assert!(!loaded.quests.has_started(BRIDGE_QUEST_ID));
    }

    #[test]
    fn unfinished_quest_and_its_materials_round_trip() {
        let mut inventory = Inventory::default();
        inventory.add(ItemKind::Axe, 1);
        inventory.add(ItemKind::Wood, 1);
        inventory.select(ItemKind::Wood);
        let mut quests = QuestLog::default();
        quests.start(BRIDGE_QUEST_ID);
        let data = save_data_from_game(
            &inventory,
            &WorldFlags::default(),
            &WorldProgress::default(),
            &quests,
        );
        let encoded = ron::to_string(&data).expect("save data serializes");
        let decoded = ron::from_str(&encoded).expect("save data deserializes");
        let loaded = game_from_save_data(decoded, &WorldMap::prototype())
            .expect("unfinished quest is restored");
        assert!(loaded.quests.has_started(BRIDGE_QUEST_ID));
        assert!(!loaded.flags.is_set(WorldFlag::BridgeBuilt));
        assert_eq!(loaded.inventory.count(ItemKind::Axe), 1);
        assert_eq!(loaded.inventory.count(ItemKind::Wood), 1);
        assert!(loaded.inventory.is_selected(ItemKind::Wood));
    }

    #[test]
    fn unknown_and_duplicate_saved_quest_ids_are_rejected() {
        let world_map = WorldMap::prototype();
        for ids in [vec![999], vec![BRIDGE_QUEST_ID.0, BRIDGE_QUEST_ID.0]] {
            let mut data = save_data_from_game(
                &Inventory::default(),
                &WorldFlags::default(),
                &WorldProgress::default(),
                &QuestLog::default(),
            );
            data.started_quest_ids = Some(ids);
            assert!(game_from_save_data(data, &world_map).is_err());
        }
    }

    #[test]
    fn current_save_without_quest_ids_is_rejected() {
        let contents = "(version:2,items:[],selected_item_id:None,bridge_built:false,\
            collected_object_ids:[],chopped_tree_ids:[])";
        let data: SaveData = ron::from_str(contents).expect("save fields deserialize");
        let error = game_from_save_data(data, &WorldMap::prototype())
            .err()
            .expect("version 2 requires quest IDs");
        assert!(error.contains("クエストID"));
    }

    #[test]
    fn save_file_round_trips_and_missing_file_starts_a_new_game() {
        let directory = env::temp_dir().join(format!(
            "hako-work-save-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock is after the Unix epoch")
                .as_nanos()
        ));
        let path = directory.join(SAVE_FILE_NAME);
        let save_file = SaveFilePath(path.clone());
        let world_map = WorldMap::prototype();

        let empty = load_game(&world_map, &save_file).expect("missing save starts a new game");
        assert_eq!(empty.inventory.count(ItemKind::Axe), 0);

        let mut inventory = Inventory::default();
        inventory.add(ItemKind::Axe, 1);
        inventory.add(ItemKind::Wood, 2);
        let mut flags = WorldFlags::default();
        flags.set(WorldFlag::BridgeBuilt, true);
        let progress = WorldProgress {
            collected_objects: HashSet::from([ObjectInstanceId(500)]),
            chopped_trees: HashSet::from([ObjectInstanceId(501)]),
        };
        let mut quests = QuestLog::default();
        quests.start(BRIDGE_QUEST_ID);
        save_game_to_path(&path, &inventory, &flags, &progress, &quests)
            .expect("save file is written");

        let loaded = load_game(&world_map, &save_file).expect("save file is restored");
        assert_eq!(loaded.inventory.count(ItemKind::Axe), 1);
        assert_eq!(loaded.inventory.count(ItemKind::Wood), 2);
        assert!(loaded.flags.is_set(WorldFlag::BridgeBuilt));
        assert!(loaded.quests.has_started(BRIDGE_QUEST_ID));
        assert!(loaded
            .progress
            .collected_objects
            .contains(&ObjectInstanceId(500)));
        assert!(loaded
            .progress
            .chopped_trees
            .contains(&ObjectInstanceId(501)));

        fs::remove_dir_all(directory).expect("test save directory is removed");
    }

    #[test]
    fn app_exit_triggers_automatic_save() {
        let directory = env::temp_dir().join(format!(
            "hako-work-exit-save-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock is after the Unix epoch")
                .as_nanos()
        ));
        let path = directory.join(SAVE_FILE_NAME);
        let mut app = App::new();
        app.add_message::<AppExit>()
            .insert_resource(SaveFilePath(path.clone()))
            .init_resource::<Inventory>()
            .init_resource::<WorldFlags>()
            .init_resource::<WorldProgress>()
            .init_resource::<QuestLog>()
            .add_systems(Update, request_app_exit)
            .add_systems(Last, save_on_exit.in_set(OnAppExitSystems));
        app.world_mut()
            .resource_mut::<Inventory>()
            .add(ItemKind::Axe, 1);
        app.world_mut()
            .resource_mut::<QuestLog>()
            .start(BRIDGE_QUEST_ID);

        app.update();

        assert!(path.exists());
        let loaded = load_game(&WorldMap::prototype(), &SaveFilePath(path.clone()))
            .expect("exit save can be loaded");
        assert_eq!(loaded.inventory.count(ItemKind::Axe), 1);
        assert!(loaded.quests.has_started(BRIDGE_QUEST_ID));
        fs::remove_dir_all(directory).expect("test save directory is removed");
    }

    fn request_app_exit(mut exits: MessageWriter<AppExit>) {
        exits.write(AppExit::Success);
    }
}
