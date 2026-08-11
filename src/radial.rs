//! Press-and-hold radial menu — how you stock the farm.
//!
//! One gesture does the whole thing: hold where you want something, flick toward what it
//! should be, release. The hold point *is* the placement point, so there's no separate
//! "now click where it goes" step and nothing to cancel.
//!
//! It shares the left mouse button with the other two verbs, which sounds like a conflict
//! and isn't — the three are cleanly separable by what your hand does:
//!
//! | gesture | verb |
//! |---|---|
//! | press, release quickly | tap the glass |
//! | press, move | shake the tank |
//! | press, hold still | this menu |
//!
//! The cost is that a tap has to be *short*, which is a real feel constraint. It's also
//! exactly how touch gestures already work, so the whole thing ports to the iPad build
//! without redesign: hold a finger on the glass, flick, lift.

use bevy::prelude::*;
use ordo::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StockItem {
    Worker,
    Queen,
    /// Arrives with foraging in M2b.
    Food,
    /// Arrives with the water simulation in M2b.
    Water,
}

impl StockItem {
    pub fn label(self) -> &'static str {
        match self {
            StockItem::Worker => "Worker",
            StockItem::Queen => "Queen",
            StockItem::Food => "Food",
            StockItem::Water => "Water",
        }
    }
}

/// Menu wedges, in screen order: up, right, down, left.
pub const WEDGES: [StockItem; 4] = [
    StockItem::Worker,
    StockItem::Queen,
    StockItem::Food,
    StockItem::Water,
];

/// What's left to place. A founding colony, which is what a real formicarium ships as:
/// one queen and a handful of workers.
#[derive(Resource)]
pub struct Stock {
    pub workers: u32,
    pub queens: u32,
}

impl Default for Stock {
    fn default() -> Self {
        Self { workers: 10, queens: 1 }
    }
}

impl Stock {
    pub fn remaining(&self, item: StockItem) -> Option<u32> {
        match item {
            StockItem::Worker => Some(self.workers),
            StockItem::Queen => Some(self.queens),
            // Not simulated yet, so deliberately not offered as a number.
            StockItem::Food | StockItem::Water => None,
        }
    }

    pub fn available(&self, item: StockItem) -> bool {
        self.remaining(item).is_some_and(|n| n > 0)
    }

    /// Put one back — used when a placement can't be honoured, so holding over solid
    /// sand costs you nothing.
    pub fn give(&mut self, item: StockItem) {
        match item {
            StockItem::Worker => self.workers += 1,
            StockItem::Queen => self.queens += 1,
            StockItem::Food | StockItem::Water => {}
        }
    }

    fn take(&mut self, item: StockItem) {
        match item {
            StockItem::Worker => self.workers = self.workers.saturating_sub(1),
            StockItem::Queen => self.queens = self.queens.saturating_sub(1),
            StockItem::Food | StockItem::Water => {}
        }
    }
}

#[derive(Resource, Default)]
pub struct RadialMenu {
    pub open: bool,
    /// Where the hold started, in screen pixels — the menu's centre.
    pub origin: Vec2,
    /// Grid cell under the hold. Where the chosen thing will go.
    pub cell: Vec2,
    pub selected: Option<usize>,
}

/// Placements waiting to be spawned, drained by [`crate::ants::place_queued`]. Going
/// through a queue keeps the input layer from needing to know how an ant is built.
#[derive(Resource, Default)]
pub struct PlacementQueue(pub Vec<(StockItem, Vec2)>);

pub fn commit_selection(
    menu: &RadialMenu,
    stock: &mut Stock,
    queue: &mut PlacementQueue,
) -> Option<StockItem> {
    let item = WEDGES[menu.selected?];
    if !stock.available(item) {
        return None;
    }
    stock.take(item);
    queue.0.push((item, menu.cell));
    Some(item)
}

// ---------------------------------------------------------------------------
// Presentation
// ---------------------------------------------------------------------------

/// Marks the spawned hub so the gesture can find it again to despawn it.
#[derive(Component)]
pub struct OpenMenu;

/// Spawn and despawn the menu to follow the gesture, and keep the hub's selection
/// in step with where the hand is pointing.
///
/// Ordo owns everything below this: where the wedges sit, which one an offset
/// points at, and what colour each ends up. All that's left here is *when* the
/// menu exists and *what's on it*, which is the part that's actually about ants.
pub fn sync_radial_ui(
    mut commands: Commands,
    menu: Res<RadialMenu>,
    stock: Res<Stock>,
    open: Query<(Entity, &mut Radial), With<OpenMenu>>,
) {
    let mut open = open;

    match (menu.open, open.iter_mut().next()) {
        (false, Some((entity, _))) => {
            commands.entity(entity).despawn();
        }
        (true, Some((_, mut hub))) => {
            if hub.selected != menu.selected {
                hub.selected = menu.selected;
            }
        }
        (true, None) => {
            let hub = commands
                .spawn((OpenMenu, radial(menu.origin, WEDGES.len()), Radial {
                    count: WEDGES.len(),
                    selected: menu.selected,
                }))
                .id();

            for (i, item) in WEDGES.iter().enumerate() {
                // The count rides in the label, so you can see what's left without
                // a separate inventory panel to read.
                let label = match stock.remaining(*item) {
                    Some(n) => format!("{} {}", item.label(), n),
                    None => item.label().to_string(),
                };
                let mut spoke = commands.spawn((wedge(i, &label), ChildOf(hub)));
                if !stock.available(*item) {
                    spoke.insert(Spent);
                }
            }
        }
        (false, None) => {}
    }
}
