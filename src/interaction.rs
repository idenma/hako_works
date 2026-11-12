use bevy::prelude::*;

use crate::inventory::{Inventory, ItemKind};
use crate::player::Player;
use crate::quest::{NotebookState, definition_for_site};
use crate::scene::{
    FlagChanged, Interactable, InteractionAction, InteractionPopup, InteractionPrompt,
    ObjectCollider, ObjectEntity, WorldFlags, WorldProgress,
};
use crate::world::{ColliderShape, ObjectInstanceId};

const INTERACTION_DISTANCE: f32 = 2.0;

pub(crate) fn is_in_interaction_range(distance: f32, collider: ColliderShape) -> bool {
    distance <= INTERACTION_DISTANCE + collider.horizontal_radius()
}

pub(crate) fn perform_interaction(// プレイヤーが対象とインタラクトする
    mut commands: Commands,
    keyboard: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    players: Query<&Transform, With<Player>>,
    interactables: Query<(
        Entity,
        &Transform,
        &Interactable,
        &ObjectEntity,
        &ObjectCollider,
    )>,
    mut inventory: ResMut<Inventory>,
    mut flags: ResMut<WorldFlags>,
    mut progress: ResMut<WorldProgress>,
    mut flag_changed: MessageWriter<FlagChanged>,
) {
    let keyboard_pressed = keyboard.just_pressed(KeyCode::KeyE);
    let gamepad_pressed = gamepads
        .iter()
        .any(|gamepad| gamepad.just_pressed(GamepadButton::South));
    if !keyboard_pressed && !gamepad_pressed {
        return;
    }

    let Ok(player_transform) = players.single() else {
        return;
    };

    let Some((_distance, object_id, entity, action)) = nearest_available_target(
        player_transform,
        interactables.iter(),
        &inventory,
        &flags,
        &progress,
    ) else {
        return;
    };

    match action {// 対象のアクションに応じてインタラクションを実行する
        InteractionAction::PickUp(item) => {
            inventory.add(item, 1);
            progress.collected_objects.insert(object_id);
            commands.entity(entity).despawn();
        }
        InteractionAction::ChopTree => {
            if !inventory.is_selected(ItemKind::Axe) || !inventory.has(ItemKind::Axe) {
                return;
            }
            progress.chopped_trees.insert(object_id);
            inventory.add(ItemKind::Wood, 1);
            commands.entity(entity).despawn();
        }
        InteractionAction::BuildBridge(flag) => {
            let quest = definition_for_site(object_id, flag);
            if !inventory.is_selected(quest.selected_item_kind())
                || !quest.consume_materials(&mut inventory)
            {
                return;
            }
            flags.set(flag, true);
            flag_changed.write(FlagChanged { flag, value: true });
            commands.entity(entity).despawn();
        }
    }
}

pub(crate) fn update_interaction_prompt(// 対話プロンプトを更新する
    players: Query<&Transform, With<Player>>,
    interactables: Query<(
        Entity,
        &Transform,
        &Interactable,
        &ObjectEntity,
        &ObjectCollider,
    )>,
    inventory: Res<Inventory>,
    notebook: Res<NotebookState>,
    flags: Res<WorldFlags>,
    progress: Res<WorldProgress>,
    mut prompt: Query<(&mut Text, &mut Visibility), With<InteractionPrompt>>,
) {
    let Ok(player_transform) = players.single() else {
        return;
    };
    let Ok((mut text, mut visibility)) = prompt.single_mut() else {
        return;
    };
    if notebook.is_open() {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    }

    let target = nearest_guided_target(
        player_transform,
        interactables.iter(),
        &inventory,
        &flags,
        &progress,
    );
    let (next_text, next_visibility) = if let Some((_, _, _, guidance)) = target {
        (guidance, Visibility::Visible)
    } else {
        (String::new(), Visibility::Hidden)
    };
    text.set_if_neq(Text::new(next_text));
    visibility.set_if_neq(next_visibility);
}

fn nearest_available_target<'a>(// プレイヤーに最も近い利用可能な対象を見つける
    player_transform: &Transform,
    interactables: impl Iterator<
        Item = (
            Entity,
            &'a Transform,
            &'a Interactable,
            &'a ObjectEntity,
            &'a ObjectCollider,
        ),
    >,
    inventory: &Inventory,
    flags: &WorldFlags,
    progress: &WorldProgress,
) -> Option<(f32, ObjectInstanceId, Entity, InteractionAction)> {
    interactables
        .filter_map(|(entity, transform, interactable, object, collider)| {
            let distance = player_transform.translation.distance(transform.translation);
            let available =
                can_perform_action(interactable.action, object.0, inventory, flags, progress);
            (is_in_interaction_range(distance, collider.0) && available)
                .then_some((distance, object.0, entity, interactable.action))
        })
        .min_by(|left, right| left.0.total_cmp(&right.0).then(left.1.cmp(&right.1)))
}

fn nearest_guided_target<'a>(// 
    player_transform: &Transform,
    interactables: impl Iterator<
        Item = (
            Entity,
            &'a Transform,
            &'a Interactable,
            &'a ObjectEntity,
            &'a ObjectCollider,
        ),
    >,
    inventory: &Inventory,
    flags: &WorldFlags,
    progress: &WorldProgress,
) -> Option<(f32, ObjectInstanceId, Entity, String)> {
    interactables
        .filter_map(|(entity, transform, interactable, object, collider)| {
            let distance = player_transform.translation.distance(transform.translation);
            let guidance =
                interaction_guidance(interactable.action, object.0, inventory, flags, progress)?;
            let available =
                can_perform_action(interactable.action, object.0, inventory, flags, progress);
            is_in_interaction_range(distance, collider.0)
                .then_some((distance, object.0, entity, guidance, available))
        })
        .min_by(|left, right| {
            right
                .4
                .cmp(&left.4)
                .then(left.0.total_cmp(&right.0))
                .then(left.1.cmp(&right.1))
        })
        .map(|(distance, object_id, entity, guidance, _)| (distance, object_id, entity, guidance))
}

fn interaction_guidance(// 対話ガイダンスを生成する
    action: InteractionAction,
    object_id: ObjectInstanceId,
    inventory: &Inventory,
    flags: &WorldFlags,
    progress: &WorldProgress,
) -> Option<String> {
    if action_is_complete(action, object_id, flags, progress) {
        return None;
    }

    Some(match action {
        InteractionAction::PickUp(item) => format!("{}を とる  [E]", item.label()),
        InteractionAction::ChopTree if !inventory.has(ItemKind::Axe) => {
            "おのを さがそう".to_owned()
        }
        InteractionAction::ChopTree if !inventory.is_selected(ItemKind::Axe) => {
            "1 おのを えらぼう".to_owned()
        }
        InteractionAction::ChopTree => "木を切る  [E]".to_owned(),
        InteractionAction::BuildBridge(flag) => {
            let quest = definition_for_site(object_id, flag);
            if let Some(missing) = quest.missing_material(inventory) {
                if missing.item_kind() != ItemKind::Wood {
                    format!("{}を あつめよう", missing.item_kind().label())
                } else if inventory.has(ItemKind::Axe) && inventory.is_selected(ItemKind::Axe) {
                    "木を切って 木ざいを とろう".to_owned()
                } else if inventory.has(ItemKind::Axe) {
                    "1 おのを えらんで 木を切ろう".to_owned()
                } else {
                    "まず おのを さがそう".to_owned()
                }
            } else if !inventory.is_selected(quest.selected_item_kind()) {
                if quest.selected_item_kind() == ItemKind::Wood {
                    "2 木ざいを えらぼう".to_owned()
                } else {
                    format!("{}を えらぼう", quest.selected_item_kind().label())
                }
            } else {
                "橋を作る  [E]".to_owned()
            }
        }
    })
}

fn action_is_complete(// 対話アクションが完了しているかどうかを確認する
    action: InteractionAction,
    object_id: ObjectInstanceId,
    flags: &WorldFlags,
    progress: &WorldProgress,
) -> bool {
    match action {
        InteractionAction::PickUp(_) => progress.collected_objects.contains(&object_id),
        InteractionAction::ChopTree => progress.chopped_trees.contains(&object_id),
        InteractionAction::BuildBridge(flag) => flags.is_set(flag),
    }
}

pub(crate) fn can_perform_action(// 対話アクションを実行できるかどうかを確認する
    action: InteractionAction,
    object_id: ObjectInstanceId,
    inventory: &Inventory,
    flags: &WorldFlags,
    progress: &WorldProgress,
) -> bool {
    if action_is_complete(action, object_id, flags, progress) {
        return false;
    }

    match action {
        InteractionAction::PickUp(_) => true,
        InteractionAction::ChopTree => {
            inventory.is_selected(ItemKind::Axe) && inventory.has(ItemKind::Axe)
        }
        InteractionAction::BuildBridge(flag) => {
            let quest = definition_for_site(object_id, flag);
            inventory.is_selected(quest.selected_item_kind())
                && quest.materials_available(inventory)
        }
    }
}

// 対象とプレイヤーの距離に応じて対象の子ポップアップを表示/非表示にする
pub(crate) fn update_interaction_popups(
    players: Query<&Transform, With<Player>>,
    interactables: Query<(&Transform, &Interactable, &ObjectEntity, &ObjectCollider)>,
    mut popups: Query<(&ChildOf, &mut Visibility), With<InteractionPopup>>,
    notebook: Res<NotebookState>,
    flags: Res<WorldFlags>,
    progress: Res<WorldProgress>,
) {
    if notebook.is_open() {
        for (_, mut visibility) in &mut popups {
            visibility.set_if_neq(Visibility::Hidden);
        }
        return;
    }
    let Ok(player_transform) = players.single() else {
        return;
    };

    for (parent, mut visibility) in &mut popups {
        let is_near = interactables.get(parent.parent()).is_ok_and(
            |(target, interactable, object, collider)| {
                let available =
                    !action_is_complete(interactable.action, object.0, &flags, &progress);
                is_in_interaction_range(
                    player_transform.translation.distance(target.translation),
                    collider.0,
                )
                    && available
            },
        );

        visibility.set_if_neq(if is_near {
            Visibility::Visible
        } else {
            Visibility::Hidden
        });
    }
}

#[cfg(test)]
mod tests {// テストモジュール
    use super::*;
    use crate::world::WorldFlag;

    #[test]
    fn tree_guidance_explains_missing_or_unselected_axe() {
        // 木のガイダンスが欠落しているか、選択されていない斧を説明することを確認する
        let tree_id = ObjectInstanceId(100);
        let flags = WorldFlags::default();
        let mut progress = WorldProgress::default();
        let mut inventory = Inventory::default();

        assert_eq!(
            interaction_guidance(
                InteractionAction::ChopTree,
                tree_id,
                &inventory,
                &flags,
                &progress,
            )
            .as_deref(),
            Some("おのを さがそう")
        );

        inventory.add(ItemKind::Axe, 1);
        inventory.add(ItemKind::Wood, 1);
        inventory.select(ItemKind::Wood);
        assert_eq!(
            interaction_guidance(
                InteractionAction::ChopTree,
                tree_id,
                &inventory,
                &flags,
                &progress,
            )
            .as_deref(),
            Some("1 おのを えらぼう")
        );

        inventory.select(ItemKind::Axe);
        assert_eq!(
            interaction_guidance(
                InteractionAction::ChopTree,
                tree_id,
                &inventory,
                &flags,
                &progress,
            )
            .as_deref(),
            Some("木を切る  [E]")
        );

        progress.chopped_trees.insert(tree_id);
        assert_eq!(
            interaction_guidance(
                InteractionAction::ChopTree,
                tree_id,
                &inventory,
                &flags,
                &progress,
            ),
            None
        );
    }

    #[test]
    fn bridge_guidance_explains_the_next_required_item() {
        // 橋のガイダンスが次に必要なアイテムを説明することを確認する
        let bridge_id = ObjectInstanceId(1);
        let bridge_flag = WorldFlag::BridgeBuilt;
        let mut flags = WorldFlags::default();
        let progress = WorldProgress::default();
        let mut inventory = Inventory::default();

        assert_eq!(
            interaction_guidance(
                InteractionAction::BuildBridge(bridge_flag),
                bridge_id,
                &inventory,
                &flags,
                &progress,
            )
            .as_deref(),
            Some("まず おのを さがそう")
        );

        inventory.add(ItemKind::Axe, 1);
        assert_eq!(
            interaction_guidance(
                InteractionAction::BuildBridge(bridge_flag),
                bridge_id,
                &inventory,
                &flags,
                &progress,
            )
            .as_deref(),
            Some("木を切って 木ざいを とろう")
        );

        inventory.add(ItemKind::Wood, 1);
        assert_eq!(
            interaction_guidance(
                InteractionAction::BuildBridge(bridge_flag),
                bridge_id,
                &inventory,
                &flags,
                &progress,
            )
            .as_deref(),
            Some("2 木ざいを えらぼう")
        );

        inventory.select(ItemKind::Wood);
        assert_eq!(
            interaction_guidance(
                InteractionAction::BuildBridge(bridge_flag),
                bridge_id,
                &inventory,
                &flags,
                &progress,
            )
            .as_deref(),
            Some("橋を作る  [E]")
        );

        flags.set(bridge_flag, true);
        assert_eq!(
            interaction_guidance(
                InteractionAction::BuildBridge(bridge_flag),
                bridge_id,
                &inventory,
                &flags,
                &progress,
            ),
            None
        );
    }
}
