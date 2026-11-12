use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use bevy::text::{Font, FontSize};

use crate::interaction::is_in_interaction_range;
use crate::inventory::{Inventory, ItemId, ItemKind};
use crate::player::Player;
use crate::scene::{
    Interactable, InteractionAction, ObjectCollider, ObjectEntity, RiversideNpc, WorldFlags,
    is_near_npc,
};
use crate::world::{BRIDGE_SITE_ID, ObjectInstanceId, ObjectKind, WorldFlag, WorldMap};

/// Stable quest catalog key; assigned values must not be changed or reused.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct QuestId(pub(crate) u16);

pub(crate) const BRIDGE_QUEST_ID: QuestId = QuestId(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RequirementRole {
    Tool,
    Material,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct QuestRequirement {// クエストの必要アイテム情報
    // item: アイテムID
    // count: 必要数
    // role: アイテムの役割（道具か材料か）
    item: ItemId,
    count: u32,
    role: RequirementRole,
}

impl QuestRequirement {// クエストの必要アイテム情報に関するメソッド
    pub(crate) fn item_kind(self) -> ItemKind {
        ItemKind::from_id(self.item)
            .unwrap_or_else(|| panic!("クエスト定義のアイテムID {} は不正です", self.item.0))
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct QuestDefinition {// クエスト定義情報
    id: QuestId,
    title: &'static str,
    target: ObjectInstanceId,
    completion_flag: WorldFlag,
    selected_item: ItemId,
    requirements: &'static [QuestRequirement],
    collecting_hint: &'static str,
    return_hint: &'static str,
}

const BRIDGE_REQUIREMENTS: [QuestRequirement; 2] = [// 橋のクエストに必要なアイテム情報
    QuestRequirement {
        item: ItemKind::Axe.id(),
        count: 1,
        role: RequirementRole::Tool,
    },
    QuestRequirement {
        item: ItemKind::Wood.id(),
        count: 1,
        role: RequirementRole::Material,
    },
];

const QUEST_DEFINITIONS: [QuestDefinition; 1] = [QuestDefinition {// クエスト定義情報
    id: BRIDGE_QUEST_ID,
    title: "はしを つくる",
    target: BRIDGE_SITE_ID,
    completion_flag: WorldFlag::BridgeBuilt,
    selected_item: ItemKind::Wood.id(),
    requirements: &BRIDGE_REQUIREMENTS,
    collecting_hint: "木を きって 木ざいを あつめよう",
    return_hint: "かわべの まるたに もどろう",
}];

impl QuestDefinition {// クエスト定義情報に関するメソッド
    pub(crate) fn selected_item_kind(&self) -> ItemKind {
        ItemKind::from_id(self.selected_item).unwrap_or_else(|| {
            panic!(
                "クエスト定義の選択アイテムID {} は不正です",
                self.selected_item.0
            )
        })
    }

    fn material_count(&self, item: ItemId) -> u32 {// 材料の必要数を取得
        self.requirements
            .iter()
            .filter(|requirement| {
                requirement.role == RequirementRole::Material && requirement.item == item
            })
            .try_fold(0_u32, |count, requirement| {
                count.checked_add(requirement.count)
            })
            .unwrap_or_else(|| panic!("クエスト {} の材料数が大きすぎます", self.id.0))
    }

    fn requirement_count(&self, requirement: QuestRequirement) -> u32 {// 指定された必要アイテムの必要数を取得
        match requirement.role {
            RequirementRole::Material => self.material_count(requirement.item),
            RequirementRole::Tool => self
                .requirements
                .iter()
                .filter(|candidate| {
                    candidate.role == RequirementRole::Tool && candidate.item == requirement.item
                })
                .map(|candidate| candidate.count)
                .max()
                .expect("tool requirement exists"),
        }
    }

    pub(crate) fn missing_material(&self, inventory: &Inventory) -> Option<QuestRequirement> {// 不足している材料を取得
        self.requirements.iter().copied().find(|requirement| {
            requirement.role == RequirementRole::Material
                && inventory.count(requirement.item_kind()) < self.material_count(requirement.item)
        })
    }

    pub(crate) fn materials_available(&self, inventory: &Inventory) -> bool {// 材料が揃っているか確認
        self.missing_material(inventory).is_none()
    }

    pub(crate) fn consume_materials(&self, inventory: &mut Inventory) -> bool {// 材料を消費し、成功したかどうかを返す
        if !self.materials_available(inventory) {
            return false;
        }

        for (index, requirement) in self.requirements.iter().enumerate() {
            if requirement.role != RequirementRole::Material
                || self.requirements[..index].iter().any(|previous| {
                    previous.role == RequirementRole::Material && previous.item == requirement.item
                })
            {
                continue;
            }

            assert!(
                inventory.consume(
                    requirement.item_kind(),
                    self.material_count(requirement.item)
                ),
                "確認済みのクエスト材料を消費できませんでした: {}",
                requirement.item.0
            );
        }
        true
    }

    fn guidance(// クエストの進行状況に応じたガイダンスを取得
        // phase: クエストの進行状況
        // inventory: プレイヤーの所持品
        // at_target: クエスト対象地点にいるか
        // notebook_open: メモが開いているか
        &self,
        phase: QuestPhase,
        inventory: &Inventory,
        at_target: bool,
        notebook_open: bool,
    ) -> String {
        match phase {
            QuestPhase::NotStarted => String::new(),
            QuestPhase::Collecting => {
                let missing_tool = self.requirements.iter().find(|requirement| {
                    requirement.role == RequirementRole::Tool
                        && inventory.count(requirement.item_kind()) < requirement.count
                });
                if let Some(requirement) = missing_tool {
                    format!("{}を さがそう", requirement.item_kind().label())
                } else {
                    self.collecting_hint.to_owned()
                }
            }
            QuestPhase::ReadyToReturn if at_target => {
                if inventory.is_selected(self.selected_item_kind()) {
                    if notebook_open {
                        "メモを とじて [E / South] つくろう".to_owned()
                    } else {
                        "[E / South] で はしを つくろう".to_owned()
                    }
                } else if notebook_open {
                    format!(
                        "メモを とじて {}を えらぼう",
                        self.selected_item_kind().label()
                    )
                } else {
                    format!("{}を えらぼう", self.selected_item_kind().label())
                }
            }
            QuestPhase::ReadyToReturn => self.return_hint.to_owned(),
            QuestPhase::Completed => "かんりょう！".to_owned(),
        }
    }
}

pub(crate) fn definition(id: QuestId) -> Option<&'static QuestDefinition> {
    QUEST_DEFINITIONS.iter().find(|quest| quest.id == id)
}

pub(crate) fn definition_for_site(// 建設地点とフラグに対応するクエスト定義を取得
    target: ObjectInstanceId,
    flag: WorldFlag,
) -> &'static QuestDefinition {
    QUEST_DEFINITIONS
        .iter()
        .find(|quest| quest.target == target && quest.completion_flag == flag)
        .unwrap_or_else(|| {
            panic!(
                "建設地点 {} とフラグ {:?} に対応するクエストがありません",
                target.0, flag
            )
        })
}

pub(crate) fn validate_definitions(world_map: &WorldMap) -> Result<(), Vec<String>> {// クエスト定義の妥当性を検証
    let mut errors = Vec::new();
    let mut quest_ids = HashSet::new();
    let mut targets = HashSet::new();

    for quest in &QUEST_DEFINITIONS {
        if quest.id.0 == 0 || !quest_ids.insert(quest.id) {
            errors.push(format!(
                "クエストID {} が不正または重複しています",
                quest.id.0
            ));
        }
        if !targets.insert(quest.target) {
            errors.push(format!(
                "クエストの建設地点 {} が重複しています",
                quest.target.0
            ));
        }

        let target = world_map
            .chunks
            .values()
            .flat_map(|chunk| &chunk.objects)
            .find(|object| object.id == quest.target);
        if !target.is_some_and(|object| {
            object.kind == ObjectKind::Log && object.completion_flag == Some(quest.completion_flag)
        }) {
            errors.push(format!("クエスト {} の建設地点が不正です", quest.id.0));
        }

        let mut material_counts = HashMap::<ItemId, u32>::new();
        for requirement in quest.requirements {
            if requirement.count == 0 || ItemKind::from_id(requirement.item).is_none() {
                errors.push(format!(
                    "クエスト {} の必要アイテムID {} または個数が不正です",
                    quest.id.0, requirement.item.0
                ));
            }
            if requirement.role == RequirementRole::Material {
                let count = material_counts.entry(requirement.item).or_default();
                if let Some(total) = count.checked_add(requirement.count) {
                    *count = total;
                } else {
                    errors.push(format!("クエスト {} の材料数が大きすぎます", quest.id.0));
                }
            }
        }
        if !material_counts.contains_key(&quest.selected_item) {
            errors.push(format!(
                "クエスト {} の選択アイテムが材料にありません",
                quest.id.0
            ));
        }
    }

    for object in world_map.chunks.values().flat_map(|chunk| &chunk.objects) {
        if object.kind == ObjectKind::Log
            && !QUEST_DEFINITIONS.iter().any(|quest| {
                quest.target == object.id && Some(quest.completion_flag) == object.completion_flag
            })
        {
            errors.push(format!(
                "建設地点 {} に対応するクエストがありません",
                object.id.0
            ));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[derive(Resource, Default)]
pub(crate) struct QuestLog {// クエストの進行状況を管理
    started: HashSet<QuestId>,
}

impl QuestLog {
    pub(crate) fn start(&mut self, id: QuestId) -> bool {
        self.started.insert(id)
    }

    pub(crate) fn has_started(&self, id: QuestId) -> bool {
        self.started.contains(&id)
    }

    pub(crate) fn started_ids(&self) -> impl Iterator<Item = QuestId> + '_ {
        self.started.iter().copied()
    }

    fn phase(
        &self,
        quest: &QuestDefinition,
        inventory: &Inventory,
        flags: &WorldFlags,
    ) -> QuestPhase {
        if flags.is_set(quest.completion_flag) {
            QuestPhase::Completed
        } else if !self.has_started(quest.id) {
            QuestPhase::NotStarted
        } else if quest.materials_available(inventory) {
            QuestPhase::ReadyToReturn
        } else {
            QuestPhase::Collecting
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum QuestPhase {
    NotStarted,
    Collecting,
    ReadyToReturn,
    Completed,
}

#[derive(Resource, Default)]
pub(crate) struct NotebookState {
    open: bool,
    block_gameplay: bool,
}

impl NotebookState {// メモの状態を管理
    pub(crate) fn is_open(&self) -> bool {
        self.open
    }
}

#[derive(Component)]
pub(crate) struct NotebookPage;

#[derive(Component)]
pub(crate) struct QuestHint;

pub(crate) fn setup_notebook(mut commands: Commands, asset_server: Res<AssetServer>) {
    let font: Handle<Font> = asset_server.load("fonts/mgenplus-1p-regular.ttf");
    commands.spawn((
        QuestHint,
        Text::new("メモ  [N / Start]"),
        TextFont {
            font: font.clone().into(),
            font_size: FontSize::Px(20.0),
            ..default()
        },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            right: px(18),
            top: px(18),
            padding: UiRect::all(px(12)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.04, 0.08, 0.08, 0.72)),
    ));

    commands.spawn((
        NotebookPage,
        Text::new(""),
        TextFont {
            font: font.into(),
            font_size: FontSize::Px(24.0),
            ..default()
        },
        TextColor(Color::srgb(0.15, 0.12, 0.08)),
        Node {
            position_type: PositionType::Absolute,
            left: percent(18),
            right: percent(18),
            top: percent(14),
            padding: UiRect::all(px(24)),
            ..default()
        },
        BackgroundColor(Color::srgb(0.96, 0.93, 0.81)),
        GlobalZIndex(10),
        Visibility::Hidden,
    ));
}

pub(crate) fn toggle_notebook(
    keyboard: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    mut notebook: ResMut<NotebookState>,
) {
    let toggled = keyboard.just_pressed(KeyCode::KeyN)
        || gamepads
            .iter()
            .any(|gamepad| gamepad.just_pressed(GamepadButton::Start));
    let block_gameplay = notebook.open || toggled;
    if notebook.block_gameplay != block_gameplay {
        notebook.block_gameplay = block_gameplay;
    }
    if toggled {
        notebook.open = !notebook.open;
    }
}

pub(crate) fn gameplay_enabled(notebook: Res<NotebookState>) -> bool {
    !notebook.block_gameplay
}

pub(crate) fn update_quests(
    players: Query<&Transform, (With<Player>, Without<RiversideNpc>)>,
    npcs: Query<&Transform, (With<RiversideNpc>, Without<Player>)>,
    flags: Res<WorldFlags>,
    mut quests: ResMut<QuestLog>,
) {
    let npc_is_near = match (players.single(), npcs.single()) {
        (Ok(player), Ok(npc)) => is_near_npc(player.translation, npc.translation),
        _ => false,
    };

    for quest in &QUEST_DEFINITIONS {
        if !quests.has_started(quest.id) && (flags.is_set(quest.completion_flag) || npc_is_near) {
            quests.start(quest.id);
        }
    }
}

pub(crate) fn update_notebook_ui(
    notebook: Res<NotebookState>,
    quests: Res<QuestLog>,
    inventory: Res<Inventory>,
    flags: Res<WorldFlags>,
    players: Query<&Transform, With<Player>>,
    interactables: Query<(&ObjectEntity, &Transform, &ObjectCollider, &Interactable)>,
    mut pages: Query<(&mut Text, &mut Visibility), (With<NotebookPage>, Without<QuestHint>)>,
    mut hints: Query<(&mut Text, &mut Visibility), (With<QuestHint>, Without<NotebookPage>)>,
) {
    let (Ok((mut page_text, mut page_visibility)), Ok((mut hint_text, mut hint_visibility))) =
        (pages.single_mut(), hints.single_mut())
    else {
        return;
    };

    let nearby_target = players.single().ok().and_then(|player| {
        interactables
            .iter()
            .filter_map(|(object, transform, collider, interactable)| {
                if !matches!(interactable.action, InteractionAction::BuildBridge(_)) {
                    return None;
                }
                let distance = player.translation.distance(transform.translation);
                is_in_interaction_range(distance, collider.0).then_some((distance, object.0))
            })
            .min_by(|left, right| left.0.total_cmp(&right.0).then(left.1.cmp(&right.1)))
            .map(|(_, object_id)| object_id)
    });

    if notebook.is_open() {
        page_text.set_if_neq(Text::new(notebook_text(
            &quests,
            &inventory,
            &flags,
            nearby_target,
        )));
        page_visibility.set_if_neq(Visibility::Visible);
        hint_visibility.set_if_neq(Visibility::Hidden);
    } else {
        hint_text.set_if_neq(Text::new(quest_hint(
            &quests,
            &inventory,
            &flags,
            nearby_target,
        )));
        page_visibility.set_if_neq(Visibility::Hidden);
        hint_visibility.set_if_neq(Visibility::Visible);
    }
}

fn notebook_text(
    quests: &QuestLog,
    inventory: &Inventory,
    flags: &WorldFlags,
    nearby_target: Option<ObjectInstanceId>,
) -> String {
    let mut text = String::from("メモちょう\n");
    let mut has_quests = false;
    for quest in &QUEST_DEFINITIONS {
        let phase = quests.phase(quest, inventory, flags);
        if phase == QuestPhase::NotStarted {
            continue;
        }
        has_quests = true;
        text.push_str(&format!("\n{}\n", quest.title));
        if phase == QuestPhase::Completed {
            text.push_str("  [OK] かんりょう！\n");
            continue;
        }

        text.push_str("  いるもの\n");
        append_requirements(&mut text, quest, inventory);
        text.push_str(&format!(
            "\n{}\n",
            quest.guidance(phase, inventory, nearby_target == Some(quest.target), true)
        ));
    }

    if !has_quests {
        text.push_str("\nまだ メモは ないよ\n");
    }
    text.push_str("\n[N / Start] とじる");
    text
}

fn append_requirements(text: &mut String, quest: &QuestDefinition, inventory: &Inventory) {
    for (index, requirement) in quest.requirements.iter().copied().enumerate() {
        if quest.requirements[..index]
            .iter()
            .any(|previous| previous.role == requirement.role && previous.item == requirement.item)
        {
            continue;
        }
        let item = requirement.item_kind();
        let owned = inventory.count(item);
        let required = quest.requirement_count(requirement);
        let marker = if owned >= required { "OK" } else { " " };
        let role = match requirement.role {
            RequirementRole::Tool => "どうぐ",
            RequirementRole::Material => "ざいりょう",
        };
        text.push_str(&format!(
            "  [{marker}] {}  {owned}/{required} ({role})\n",
            item.label()
        ));
    }
}

fn quest_hint(
    quests: &QuestLog,
    inventory: &Inventory,
    flags: &WorldFlags,
    nearby_target: Option<ObjectInstanceId>,
) -> String {
    let mut hint = String::from("メモ  [N / Start]");
    if let Some((quest, phase)) = QUEST_DEFINITIONS.iter().find_map(|quest| {
        let phase = quests.phase(quest, inventory, flags);
        matches!(phase, QuestPhase::Collecting | QuestPhase::ReadyToReturn)
            .then_some((quest, phase))
    }) {
        hint.push_str(&format!(
            "\n{}\n{}",
            quest.title,
            quest.guidance(phase, inventory, nearby_target == Some(quest.target), false)
        ));
    }
    hint
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_quest_definition_and_ids_are_valid() {
        assert_eq!(BRIDGE_QUEST_ID, QuestId(1));
        assert!(definition(BRIDGE_QUEST_ID).is_some());
        assert!(definition(QuestId(0)).is_none());
        assert_eq!(validate_definitions(&WorldMap::prototype()), Ok(()));
    }

    #[test]
    fn nearby_npc_starts_the_quest_and_it_remains_started() {
        let mut app = App::new();
        app.init_resource::<QuestLog>()
            .init_resource::<WorldFlags>()
            .add_systems(Update, update_quests);
        let player = app
            .world_mut()
            .spawn((Player::default(), Transform::from_xyz(20.0, 0.0, 0.0)))
            .id();
        app.world_mut()
            .spawn((RiversideNpc, Transform::from_xyz(3.2, 0.0, 0.0)));
        app.update();
        assert!(
            !app.world()
                .resource::<QuestLog>()
                .has_started(BRIDGE_QUEST_ID)
        );

        app.world_mut()
            .get_mut::<Transform>(player)
            .expect("player exists")
            .translation = Vec3::new(0.0, 0.0, 0.0);
        app.update();
        assert!(
            app.world()
                .resource::<QuestLog>()
                .has_started(BRIDGE_QUEST_ID)
        );

        app.world_mut()
            .get_mut::<Transform>(player)
            .expect("player exists")
            .translation = Vec3::new(20.0, 0.0, 0.0);
        app.update();
        assert!(
            app.world()
                .resource::<QuestLog>()
                .has_started(BRIDGE_QUEST_ID)
        );
    }

    #[test]
    fn quest_phase_tracks_materials_and_the_bridge_flag() {
        let quest = definition(BRIDGE_QUEST_ID).expect("bridge quest exists");
        let mut log = QuestLog::default();
        let mut inventory = Inventory::default();
        let mut flags = WorldFlags::default();
        assert_eq!(log.phase(quest, &inventory, &flags), QuestPhase::NotStarted);
        log.start(BRIDGE_QUEST_ID);
        assert_eq!(log.phase(quest, &inventory, &flags), QuestPhase::Collecting);

        inventory.add(ItemKind::Wood, 1);
        assert_eq!(
            log.phase(quest, &inventory, &flags),
            QuestPhase::ReadyToReturn
        );
        assert!(inventory.consume(ItemKind::Wood, 1));
        assert_eq!(log.phase(quest, &inventory, &flags), QuestPhase::Collecting);

        flags.set(WorldFlag::BridgeBuilt, true);
        assert_eq!(log.phase(quest, &inventory, &flags), QuestPhase::Completed);
    }

    #[test]
    fn multiple_materials_are_checked_before_any_are_consumed() {
        const REQUIREMENTS: [QuestRequirement; 2] = [
            QuestRequirement {
                item: ItemKind::Axe.id(),
                count: 1,
                role: RequirementRole::Material,
            },
            QuestRequirement {
                item: ItemKind::Wood.id(),
                count: 2,
                role: RequirementRole::Material,
            },
        ];
        let quest = QuestDefinition {
            requirements: &REQUIREMENTS,
            ..QUEST_DEFINITIONS[0]
        };
        let mut inventory = Inventory::default();
        inventory.add(ItemKind::Axe, 1);
        inventory.add(ItemKind::Wood, 1);
        assert!(!quest.consume_materials(&mut inventory));
        assert_eq!(inventory.count(ItemKind::Axe), 1);
        assert_eq!(inventory.count(ItemKind::Wood), 1);

        inventory.add(ItemKind::Wood, 1);
        assert!(quest.consume_materials(&mut inventory));
        assert_eq!(inventory.count(ItemKind::Axe), 0);
        assert_eq!(inventory.count(ItemKind::Wood), 0);
    }

    #[test]
    fn duplicate_material_requirements_are_summed_before_consumption() {
        const REQUIREMENTS: [QuestRequirement; 2] = [
            QuestRequirement {
                item: ItemKind::Wood.id(),
                count: 1,
                role: RequirementRole::Material,
            },
            QuestRequirement {
                item: ItemKind::Wood.id(),
                count: 1,
                role: RequirementRole::Material,
            },
        ];
        let quest = QuestDefinition {
            requirements: &REQUIREMENTS,
            ..QUEST_DEFINITIONS[0]
        };
        let mut inventory = Inventory::default();
        inventory.add(ItemKind::Wood, 1);
        let mut text = String::new();
        append_requirements(&mut text, &quest, &inventory);
        assert!(text.contains("木ざい  1/2 (ざいりょう)"));
        assert_eq!(text.matches("木ざい").count(), 1);
        assert!(!quest.consume_materials(&mut inventory));
        assert_eq!(inventory.count(ItemKind::Wood), 1);
        inventory.add(ItemKind::Wood, 1);
        assert!(quest.consume_materials(&mut inventory));
        assert_eq!(inventory.count(ItemKind::Wood), 0);
    }

    #[test]
    fn notebook_lists_requirements_return_guidance_and_completion() {
        let mut log = QuestLog::default();
        let mut inventory = Inventory::default();
        let mut flags = WorldFlags::default();
        assert!(notebook_text(&log, &inventory, &flags, None).contains("まだ メモは ないよ"));

        log.start(BRIDGE_QUEST_ID);
        let text = notebook_text(&log, &inventory, &flags, None);
        assert!(text.contains("おの  0/1 (どうぐ)"));
        assert!(text.contains("木ざい  0/1 (ざいりょう)"));
        assert!(text.contains("おのを さがそう"));

        inventory.add(ItemKind::Axe, 1);
        inventory.add(ItemKind::Wood, 1);
        let text = notebook_text(&log, &inventory, &flags, None);
        assert!(text.contains("[OK] おの"));
        assert!(text.contains("かわべの まるたに もどろう"));
        assert!(quest_hint(&log, &inventory, &flags, None).contains("かわべの まるたに もどろう"));
        assert!(
            notebook_text(&log, &inventory, &flags, Some(BRIDGE_SITE_ID))
                .contains("メモを とじて 木ざいを えらぼう")
        );
        inventory.select(ItemKind::Wood);
        assert!(
            notebook_text(&log, &inventory, &flags, Some(BRIDGE_SITE_ID))
                .contains("メモを とじて [E / South] つくろう")
        );

        flags.set(WorldFlag::BridgeBuilt, true);
        let text = notebook_text(&log, &inventory, &flags, None);
        assert!(text.contains("はしを つくる"));
        assert!(text.contains("かんりょう！"));
    }

    #[test]
    fn notebook_toggles_with_gamepad_and_blocks_the_closing_frame() {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<NotebookState>()
            .add_systems(Update, toggle_notebook);
        let gamepad = app.world_mut().spawn(Gamepad::default()).id();
        app.world_mut()
            .get_mut::<Gamepad>(gamepad)
            .expect("gamepad exists")
            .digital_mut()
            .press(GamepadButton::Start);
        app.update();
        assert!(app.world().resource::<NotebookState>().open);
        assert!(app.world().resource::<NotebookState>().block_gameplay);

        let mut gamepad_input = app
            .world_mut()
            .get_mut::<Gamepad>(gamepad)
            .expect("gamepad exists");
        gamepad_input.digital_mut().reset_all();
        gamepad_input.digital_mut().press(GamepadButton::Start);
        drop(gamepad_input);
        app.update();
        assert!(!app.world().resource::<NotebookState>().open);
        assert!(app.world().resource::<NotebookState>().block_gameplay);

        app.world_mut()
            .get_mut::<Gamepad>(gamepad)
            .expect("gamepad exists")
            .digital_mut()
            .reset_all();
        app.update();
        assert!(!app.world().resource::<NotebookState>().block_gameplay);
    }

    #[test]
    fn notebook_ui_shows_the_page_or_return_hint() {
        let mut app = App::new();
        app.init_resource::<NotebookState>()
            .init_resource::<QuestLog>()
            .init_resource::<Inventory>()
            .init_resource::<WorldFlags>()
            .add_systems(Update, update_notebook_ui);
        let page = app
            .world_mut()
            .spawn((NotebookPage, Text::new(""), Visibility::Hidden))
            .id();
        let hint = app
            .world_mut()
            .spawn((QuestHint, Text::new(""), Visibility::Hidden))
            .id();
        app.world_mut()
            .resource_mut::<QuestLog>()
            .start(BRIDGE_QUEST_ID);
        app.world_mut()
            .resource_mut::<Inventory>()
            .add(ItemKind::Wood, 1);
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(page).expect("page exists"),
            Visibility::Hidden
        );
        assert_eq!(
            *app.world().get::<Visibility>(hint).expect("hint exists"),
            Visibility::Visible
        );
        assert!(
            app.world()
                .get::<Text>(hint)
                .expect("hint exists")
                .0
                .contains("かわべの まるたに もどろう")
        );
        app.world_mut().clear_trackers();
        app.update();
        assert!(
            !app.world()
                .entity(hint)
                .get_ref::<Text>()
                .expect("hint exists")
                .is_changed()
        );

        app.world_mut().resource_mut::<NotebookState>().open = true;
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(page).expect("page exists"),
            Visibility::Visible
        );
        assert_eq!(
            *app.world().get::<Visibility>(hint).expect("hint exists"),
            Visibility::Hidden
        );
        assert!(
            app.world()
                .get::<Text>(page)
                .expect("page exists")
                .0
                .contains("木ざい  1/1")
        );

        app.world_mut()
            .spawn((Player::default(), Transform::default()));
        app.world_mut().spawn((
            ObjectEntity(BRIDGE_SITE_ID),
            ObjectCollider(crate::world::ColliderShape::Box {
                half_extents: Vec3::splat(0.5),
            }),
            Interactable {
                action: InteractionAction::BuildBridge(WorldFlag::BridgeBuilt),
            },
            Transform::default(),
        ));
        app.update();
        assert!(
            app.world()
                .get::<Text>(page)
                .expect("page exists")
                .0
                .contains("メモを とじて [E / South] つくろう")
        );

        app.world_mut().resource_mut::<NotebookState>().open = false;
        app.update();
        assert!(
            app.world()
                .get::<Text>(hint)
                .expect("hint exists")
                .0
                .contains("[E / South] で はしを つくろう")
        );
    }

    #[test]
    fn notebook_hides_other_gameplay_prompts_until_closed() {
        use crate::interaction::{update_interaction_popups, update_interaction_prompt};
        use crate::inventory::{InventoryHud, update_inventory_hud};
        use crate::scene::{
            InteractionPopup, InteractionPrompt, NpcDialogueHud, WorldProgress,
            update_riverside_npc,
        };
        use crate::world::ColliderShape;

        let mut app = App::new();
        app.init_resource::<NotebookState>()
            .init_resource::<Inventory>()
            .init_resource::<WorldFlags>()
            .init_resource::<WorldProgress>()
            .add_systems(
                Update,
                (
                    update_inventory_hud,
                    update_riverside_npc,
                    update_interaction_prompt,
                    update_interaction_popups,
                ),
            );
        app.world_mut()
            .spawn((Player::default(), Transform::from_xyz(0.0, 0.6, 0.0)));
        app.world_mut()
            .spawn((RiversideNpc, Transform::from_xyz(3.2, 0.0, 0.0)));
        let inventory_hud = app
            .world_mut()
            .spawn((InventoryHud, Text::new(""), Visibility::Hidden))
            .id();
        let dialogue = app
            .world_mut()
            .spawn((NpcDialogueHud, Text::new(""), Visibility::Hidden))
            .id();
        let prompt = app
            .world_mut()
            .spawn((InteractionPrompt, Text::new(""), Visibility::Hidden))
            .id();
        let target = app
            .world_mut()
            .spawn((
                ObjectEntity(ObjectInstanceId(500)),
                ObjectCollider(ColliderShape::Box {
                    half_extents: Vec3::splat(0.5),
                }),
                Interactable {
                    action: InteractionAction::PickUp(ItemKind::Axe),
                },
                Transform::from_xyz(1.0, 0.0, 0.0),
            ))
            .id();
        let popup = app
            .world_mut()
            .spawn((InteractionPopup, ChildOf(target), Visibility::Hidden))
            .id();

        app.update();
        for entity in [inventory_hud, dialogue, prompt, popup] {
            assert_eq!(
                *app.world().get::<Visibility>(entity).expect("HUD exists"),
                Visibility::Visible
            );
        }
        app.world_mut().resource_mut::<NotebookState>().open = true;
        app.update();
        for entity in [inventory_hud, dialogue, prompt, popup] {
            assert_eq!(
                *app.world().get::<Visibility>(entity).expect("HUD exists"),
                Visibility::Hidden
            );
        }
        app.world_mut().resource_mut::<NotebookState>().open = false;
        app.update();
        for entity in [inventory_hud, dialogue, prompt, popup] {
            assert_eq!(
                *app.world().get::<Visibility>(entity).expect("HUD exists"),
                Visibility::Visible
            );
        }
    }

    #[test]
    fn notebook_stops_player_movement_and_jump_until_after_closing() {
        use crate::player::{PLAYER_GROUND_Y, move_player};
        use std::time::Duration;

        let mut app = App::new();
        app.insert_resource(WorldMap::prototype())
            .init_resource::<WorldFlags>()
            .init_resource::<NotebookState>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<Time>()
            .add_systems(
                Update,
                (toggle_notebook, move_player.run_if(gameplay_enabled)).chain(),
            );
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_millis(100));
        let player = app
            .world_mut()
            .spawn((
                Player::default(),
                Transform::from_xyz(0.0, PLAYER_GROUND_Y, 0.0),
            ))
            .id();

        let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        input.press(KeyCode::KeyN);
        input.press(KeyCode::KeyW);
        input.press(KeyCode::Space);
        drop(input);
        app.update();
        assert_eq!(
            app.world()
                .get::<Transform>(player)
                .expect("player exists")
                .translation,
            Vec3::new(0.0, PLAYER_GROUND_Y, 0.0)
        );

        let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        input.reset_all();
        input.press(KeyCode::KeyN);
        input.press(KeyCode::KeyW);
        drop(input);
        app.update();
        assert_eq!(
            app.world()
                .get::<Transform>(player)
                .expect("player exists")
                .translation,
            Vec3::new(0.0, PLAYER_GROUND_Y, 0.0)
        );

        let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        input.reset_all();
        input.press(KeyCode::KeyW);
        drop(input);
        app.update();
        assert!(
            app.world()
                .get::<Transform>(player)
                .expect("player exists")
                .translation
                .z
                < 0.0
        );
    }
}
