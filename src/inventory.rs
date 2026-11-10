use std::collections::HashMap;

use bevy::prelude::*;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum ItemKind {
    Axe,
    Wood,
}

/// Stable item catalog key; assigned values must not be changed or reused.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ItemId(pub(crate) u16);

impl ItemKind {
    const ALL: [Self; 2] = [Self::Axe, Self::Wood];

    pub(crate) const fn id(self) -> ItemId {
        match self {
            Self::Axe => ItemId(1),
            Self::Wood => ItemId(2),
        }
    }

    pub(crate) const fn from_id(id: ItemId) -> Option<Self> {
        match id.0 {
            1 => Some(Self::Axe),
            2 => Some(Self::Wood),
            _ => None,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Axe => "おの",
            Self::Wood => "木ざい",
        }
    }
}

#[derive(Resource, Default)]
pub(crate) struct Inventory {// プレイヤーのインベントリを管理する
    pub(crate) items: HashMap<ItemId, u32>,
    pub(crate) selected: Option<ItemId>,
}

impl Inventory {
    pub(crate) fn count(&self, item: ItemKind) -> u32 {
        self.items.get(&item.id()).copied().unwrap_or(0)
    }

    pub(crate) fn has(&self, item: ItemKind) -> bool {
        self.count(item) > 0
    }

    pub(crate) fn add(&mut self, item: ItemKind, amount: u32) {
        *self.items.entry(item.id()).or_default() += amount;
        if self.selected.is_none() {
            self.selected = Some(item.id());
        }
    }

    pub(crate) fn consume(&mut self, item: ItemKind, amount: u32) -> bool {
        let depleted = {
            let Some(count) = self.items.get_mut(&item.id()) else {
                return false;
            };
            if *count < amount {
                return false;
            }
            *count -= amount;
            *count == 0
        };
        if depleted {
            self.items.remove(&item.id());
        }
        if depleted && self.selected == Some(item.id()) {
            self.selected = ItemKind::ALL
                .into_iter()
                .find(|candidate| self.has(*candidate))
                .map(ItemKind::id);
        }
        true
    }

    pub(crate) fn select(&mut self, item: ItemKind) {
        if self.has(item) {
            self.selected = Some(item.id());
        }
    }

    pub(crate) fn is_selected(&self, item: ItemKind) -> bool {
        self.selected == Some(item.id())
    }
}

#[derive(Component)]
pub(crate) struct InventoryHud;

pub(crate) fn select_inventory_item(// インベントリのアイテムを選択する
    keyboard: Res<ButtonInput<KeyCode>>,
    mut inventory: ResMut<Inventory>,
) {
    if keyboard.just_pressed(KeyCode::Digit1) {
        inventory.select(ItemKind::Axe);
    }
    if keyboard.just_pressed(KeyCode::Digit2) {
        inventory.select(ItemKind::Wood);
    }
}

pub(crate) fn update_inventory_hud(// インベントリHUDを更新する
    inventory: Res<Inventory>,
    mut hud: Query<&mut Text, With<InventoryHud>>,
) {
    let Ok(mut text) = hud.single_mut() else {
        return;
    };

    let axe_marker = if inventory.is_selected(ItemKind::Axe) {
        ">"
    } else {
        " "
    };
    let wood_marker = if inventory.is_selected(ItemKind::Wood) {
        ">"
    } else {
        " "
    };
    text.0 = format!(
        "もちもの\n{axe_marker}1 {}  {}\n{wood_marker}2 {} {}\n1/2 えらぶ",
        ItemKind::Axe.label(),
        inventory.count(ItemKind::Axe),
        ItemKind::Wood.label(),
        inventory.count(ItemKind::Wood),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_ids_are_stable_and_round_trip_to_their_kinds() {
        assert_eq!(ItemKind::Axe.id(), ItemId(1));
        assert_eq!(ItemKind::Wood.id(), ItemId(2));
        assert_eq!(ItemKind::from_id(ItemId(1)), Some(ItemKind::Axe));
        assert_eq!(ItemKind::from_id(ItemId(2)), Some(ItemKind::Wood));
        assert_eq!(ItemKind::from_id(ItemId(0)), None);
        assert_ne!(ItemKind::Axe.id(), ItemKind::Wood.id());
    }
}
