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

/// Screen-space radius the wedges sit at, and the dead zone in the middle where nothing
/// is selected yet — so opening the menu doesn't instantly commit to whatever direction
/// your hand drifted.
pub const WEDGE_RADIUS: f32 = 84.0;
pub const DEAD_ZONE: f32 = 26.0;

/// Which wedge a cursor offset points at. Screen space, so `y` grows downward.
pub fn wedge_at(offset: Vec2) -> Option<usize> {
    if offset.length() < DEAD_ZONE {
        return None;
    }
    Some(if offset.x.abs() > offset.y.abs() {
        if offset.x > 0.0 { 1 } else { 3 }
    } else if offset.y > 0.0 {
        2
    } else {
        0
    })
}

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

#[derive(Component)]
pub struct RadialUi;

#[derive(Component)]
pub struct RadialItem(pub usize);

const IDLE: Color = Color::srgba(0.90, 0.88, 0.83, 0.72);
const PICKED: Color = Color::srgb(1.0, 0.97, 0.90);
const SPENT: Color = Color::srgba(0.62, 0.60, 0.57, 0.35);

/// Spawn and despawn the menu's UI to follow the gesture, and keep the highlight in step
/// with where the hand is pointing.
pub fn sync_radial_ui(
    mut commands: Commands,
    menu: Res<RadialMenu>,
    stock: Res<Stock>,
    existing: Query<Entity, With<RadialUi>>,
    mut items: Query<(&RadialItem, &mut TextColor)>,
) {
    let shown = existing.iter().next();

    match (menu.open, shown) {
        (false, Some(root)) => {
            commands.entity(root).despawn();
        }
        (true, None) => spawn_radial(&mut commands, &menu, &stock),
        (true, Some(_)) => {
            for (item, mut colour) in &mut items {
                colour.0 = wedge_colour(WEDGES[item.0], &stock, menu.selected == Some(item.0));
            }
        }
        (false, None) => {}
    }
}

fn wedge_colour(item: StockItem, stock: &Stock, selected: bool) -> Color {
    if !stock.available(item) {
        SPENT
    } else if selected {
        PICKED
    } else {
        IDLE
    }
}

fn spawn_radial(commands: &mut Commands, menu: &RadialMenu, stock: &Stock) {
    // Offsets match WEDGES: up, right, down, left.
    const DIRS: [Vec2; 4] = [
        Vec2::new(0.0, -1.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(-1.0, 0.0),
    ];

    let root = commands
        .spawn((
            RadialUi,
            Node {
                position_type: PositionType::Absolute,
                left: px(menu.origin.x),
                top: px(menu.origin.y),
                ..default()
            },
        ))
        .id();

    for (i, dir) in DIRS.iter().enumerate() {
        let item = WEDGES[i];
        let label = match stock.remaining(item) {
            Some(n) => format!("{} {}", item.label(), n),
            None => item.label().to_string(),
        };
        let offset = *dir * WEDGE_RADIUS;

        commands.spawn((
            RadialItem(i),
            ChildOf(root),
            Node {
                position_type: PositionType::Absolute,
                // Nudged so each label sits roughly centred on its point.
                left: px(offset.x - 34.0),
                top: px(offset.y - 10.0),
                ..default()
            },
            Text::new(label),
            TextFont { font_size: FontSize::Px(17.0), ..default() },
            TextColor(wedge_colour(item, stock, menu.selected == Some(i))),
        ));
    }
}
