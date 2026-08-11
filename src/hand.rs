//! The hand.
//!
//! Lifted from Divus Factus, where it is the god's own hand and the game's primary verb.
//! The recipe is the same — a rig of cubes with jointed fingers, one scalar per pose, drawn
//! by an overlay camera so it is never occluded by anything, world or interface. What
//! changes is what it's *for*. There it grabs the ground to turn the world. Here there is
//! only glass, and two things you can do to it.
//!
//! That mapping is the whole design:
//!
//! - **A fingertip on the glass.** The index reaches out and presses, and the hand leans in
//!   after it. This is a tap: the innocent thing, the thing the game says nothing about.
//! - **A palm on the glass.** Fingers splay, the hand flattens and plants. This is the grab,
//!   the same gesture as seizing the ground in Divus Factus, and from here the tank goes
//!   wherever your arm does.
//!
//! Which is a better sign than any label could be. The game asks you not to shake it; the
//! difference between a finger and a whole hand is legible before you've done either.
//!
//! Nothing here reads the mouse. [`Touch`] says what the hand is *doing* and the input path
//! fills it in, which keeps the iPad door open: a finger on a screen already knows whether
//! it is tapping or dragging, and it would write the same three fields.

use std::f32::consts::FRAC_PI_2;

use bevy::camera::visibility::RenderLayers;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::prelude::*;
use bevy::window::{CursorOptions, PrimaryWindow};

use crate::tank::TankCamera;
use crate::title::GameState;

/// Render layer the hand occupies. Anything on it draws above the world and the interface.
pub const HAND_LAYER: usize = 1;

/// How far in front of the camera the hand floats, and how big it is there.
///
/// The camera sits at `CAM_DIST` and never moves, so unlike Divus Factus — where the hand
/// has to grow and shrink with a zoom — one distance and one scale do for the whole game.
const HAND_DEPTH: f32 = 6.0;
const HAND_SCALE: f32 = 0.5;

/// How far the hand presses toward the glass, in world units, for a fingertip and for a
/// planted palm. Small: it's the finger that does the acting, and this sells the
/// follow-through.
const PRESS_DEPTH: f32 = 0.22;
const PLANT_DEPTH: f32 = 0.34;

/// Pose easing, per second. A press has a fast attack and a slow release, because a
/// fingertip lands sharply and lifts like a finger rather than a switch.
const PRESS_ATTACK: f32 = 34.0;
const PRESS_RELEASE: f32 = 11.0;
const GRAB_EASE: f32 = 18.0;
const SETTLE: f32 = 7.0;

/// How fast the hand converges on the cursor. Deliberately not instant — a hand that
/// tracked rigidly would read as a dragged icon — but fast enough that it never feels
/// like it missed.
const FOLLOW: f32 = 26.0;

/// Marks the root the whole rig hangs from.
#[derive(Component)]
pub struct HandModel;

/// The overlay camera. A child of the tank camera with an identity transform, which is what
/// keeps the two views pixel-aligned with no sync system.
#[derive(Component)]
pub struct HandCamera;

/// The hand's two materials, kept so its skin can be changed while the game runs.
///
/// Two handles rather than a colour per mesh: every finger bone shares one of these, so a
/// change lands on the whole hand at once and can't leave a joint behind.
#[derive(Resource)]
pub struct HandMaterials {
    pub skin: Handle<StandardMaterial>,
    pub knuckle: Handle<StandardMaterial>,
}

/// What the hand is doing. Written by the input path each frame; read here.
///
/// Deliberately about intent rather than devices. A touchscreen would fill in exactly these
/// three fields — see the module note.
#[derive(Resource, Default)]
pub struct Touch {
    /// Where the pointer is, in window pixels. `None` when it has left the window, and the
    /// hand goes with it.
    pub at: Option<Vec2>,
    /// A fingertip on the glass: a tap, or the first moment of one.
    pub pressing: bool,
    /// A palm on the glass, dragging the tank about.
    pub grabbing: bool,
}

/// The rig's joints, and its pose.
#[derive(Component)]
pub struct HandRig {
    /// `[knuckle, mid-joint]` per finger, index to little.
    fingers: Vec<[Entity; 2]>,
    /// `[base, tip]`.
    thumb: [Entity; 2],
    /// Fingertip press, 0 raised to 1 pressed.
    press: f32,
    /// Palm plant, 0 off the glass to 1 flat against it. Splays the fingers.
    grab: f32,
    /// Smoothed lean from the hand's own motion, `(roll, pitch)`. More than any idle loop,
    /// this is what makes the hand read as suspended rather than pinned.
    bank: Vec2,
}

pub fn setup_hand(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    camera: Query<Entity, With<TankCamera>>,
) {
    let Ok(tank_camera) = camera.single() else {
        // Nothing to hang the overlay off. Loud, because the alternative is a game with no
        // cursor at all and no clue why.
        error!("no tank camera, so no hand");
        return;
    };

    // The overlay camera. `order: 1` puts it after the tank camera has composited the world
    // *and* the interface, so the hand glides over the radial wheel the way it glides over
    // sand. No clear, or it would wipe that image out.
    commands.spawn((
        HandCamera,
        Camera3d::default(),
        Camera {
            order: 1,
            clear_color: ClearColorConfig::None,
            ..default()
        },
        // Tonemapping off. The image this pass writes over has already been through the tank
        // camera's curve, and running it a second time is what turned the whole window
        // black the moment the hand had anything to draw. A cursor is not scenery: it goes
        // down raw on top of a finished frame.
        Tonemapping::None,
        RenderLayers::layer(HAND_LAYER),
        Transform::IDENTITY,
        ChildOf(tank_camera),
    ));

    // The hand's own light, on the hand's own layer. The tank's two lights are on layer 0
    // and don't reach here, and an unlit hand is a black hand.
    //
    // Much dimmer than the tank's key light, because the global ambient reaches this layer
    // too and the two together blew the hand out to a paper cutout at the tank's own
    // illuminance. Angled from the upper left, like the tank's, so the hand is lit by the
    // same imaginary window.
    commands.spawn((
        DirectionalLight {
            illuminance: 1900.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(-4.0, 6.0, 8.0).looking_at(Vec3::ZERO, Vec3::Y),
        RenderLayers::layer(HAND_LAYER),
    ));

    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));

    // Skin, and a shade darker at the knuckles so the joints read at this size. The colours
    // are placeholders: `restyle_hand` writes the player's chosen tone over them on the first
    // frame, and these are only what the hand looks like before it has run.
    //
    // Barely emissive. Enough that the hand never disappears into a dark stratum, not
    // enough to glow — the first cut carried ten times this and the hand came out as a white
    // paper cutout laid over the farm rather than a thing in the room with it.
    let skin = materials.add(StandardMaterial {
        base_color: Color::srgb(0.82, 0.63, 0.52),
        emissive: LinearRgba::new(0.012, 0.007, 0.005, 1.0),
        perceptual_roughness: 0.78,
        reflectance: 0.04,
        ..default()
    });
    let knuckle = materials.add(StandardMaterial {
        base_color: Color::srgb(0.70, 0.51, 0.42),
        emissive: LinearRgba::new(0.008, 0.005, 0.004, 1.0),
        perceptual_roughness: 0.82,
        reflectance: 0.03,
        ..default()
    });

    commands.insert_resource(HandMaterials {
        skin: skin.clone(),
        knuckle: knuckle.clone(),
    });

    let root = commands
        .spawn((HandModel, Transform::default(), Visibility::Hidden))
        .id();

    // Palm. Fingers hang off its leading edge; forward is -Z. `RenderLayers` doesn't
    // inherit, so every mesh carries its own.
    commands.spawn((
        Mesh3d(cube.clone()),
        MeshMaterial3d(skin.clone()),
        Transform::from_xyz(0.0, 0.0, 0.12).with_scale(Vec3::new(1.0, 0.26, 1.08)),
        RenderLayers::layer(HAND_LAYER),
        ChildOf(root),
    ));

    // A finger segment: a joint entity at the knuckle with the bone hanging off it, so
    // rotating the joint curls the finger.
    let mut segment = |parent: Entity,
                       joint: Vec3,
                       yaw: f32,
                       length: f32,
                       girth: f32,
                       material: &Handle<StandardMaterial>|
     -> Entity {
        let joint_entity = commands
            .spawn((
                Transform::from_translation(joint).with_rotation(Quat::from_rotation_y(yaw)),
                Visibility::default(),
                ChildOf(parent),
            ))
            .id();
        commands.spawn((
            Mesh3d(cube.clone()),
            MeshMaterial3d(material.clone()),
            Transform::from_xyz(0.0, 0.0, -length * 0.5).with_scale(Vec3::new(
                girth,
                girth * 1.05,
                length,
            )),
            RenderLayers::layer(HAND_LAYER),
            ChildOf(joint_entity),
        ));
        joint_entity
    };

    // Four fingers of different lengths — even this blocky, equal fingers read as a rake
    // rather than a hand.
    let mut fingers = Vec::with_capacity(4);
    for (x, length) in [(-0.36, 0.52), (-0.12, 0.62), (0.12, 0.57), (0.36, 0.42)] {
        let proximal = segment(root, Vec3::new(x, 0.0, -0.42), 0.0, length, 0.19, &skin);
        let distal = segment(
            proximal,
            Vec3::new(0.0, 0.0, -length),
            0.0,
            length * 0.8,
            0.17,
            &knuckle,
        );
        fingers.push([proximal, distal]);
    }

    // Thumb: on the side, splayed outward, opposing the fingers.
    //
    // Tucked *inside* the palm's left edge rather than just beyond it. Divus Factus looks
    // at its hand from above and slightly behind, where a thumb standing off the edge reads
    // as a thumb; this game looks at it dead-on, and out there it read as a separate little
    // brick floating beside the hand.
    let thumb_base = segment(root, Vec3::new(-0.46, -0.02, 0.26), 0.95, 0.46, 0.2, &skin);
    let thumb_tip = segment(thumb_base, Vec3::new(0.0, 0.0, -0.46), 0.0, 0.36, 0.18, &knuckle);

    commands.entity(root).insert(HandRig {
        fingers,
        thumb: [thumb_base, thumb_tip],
        press: 0.0,
        grab: 0.0,
        bank: Vec2::ZERO,
    });
}

/// The rotation that turns the flat rig into a cursor: back of the hand to the viewer,
/// fingers reaching up-screen, leaning the way every cursor has leaned since the first.
///
/// Rotating local +Y (the palm's normal) onto the camera's +Z faces the back of the hand at
/// the viewer and swings the fingers — which run along local -Z — up the screen.
fn cursor_rotation(camera: &GlobalTransform, lean: Vec2, grab: f32) -> Quat {
    camera.rotation()
        * Quat::from_rotation_x(FRAC_PI_2)
        // Flattening onto the glass takes the lean out: a planted palm squares up to what
        // it is holding.
        * Quat::from_rotation_y(0.25 * (1.0 - grab) + lean.x)
        * Quat::from_rotation_x(-0.15 * (1.0 - grab) + lean.y)
}

/// Where the fingertip is, in the rig's own frame, given how far the index has curled.
///
/// The *tip* is parked on the cursor rather than the palm, and it's computed from the real
/// joint chain so it stays glued to the point mid-press instead of sliding out from under
/// it as the finger bends.
fn fingertip(press: f32) -> Vec3 {
    let proximal = 0.04 + press * 0.42;
    let distal = proximal + 0.03 + press * 0.5;
    Vec3::new(
        -0.36,
        -(0.52 * proximal.sin() + 0.416 * distal.sin()),
        -(0.42 + 0.52 * proximal.cos() + 0.416 * distal.cos()),
    ) * HAND_SCALE
}

pub fn move_hand(
    time: Res<Time<Real>>,
    touch: Res<Touch>,
    state: Res<State<GameState>>,
    camera: Single<(&Camera, &GlobalTransform), With<TankCamera>>,
    mut hand: Single<(&mut Transform, &mut Visibility, &mut HandRig), With<HandModel>>,
    mut joints: Query<&mut Transform, Without<HandModel>>,
) {
    let dt = time.delta_secs().max(1.0 / 240.0);
    let (camera, camera_tf) = *camera;
    let (transform, visibility, rig) = &mut *hand;

    // No pointer, or a screen the hand has no business being on — the studio's mark is
    // black and nothing else.
    let showing = touch.at.is_some() && *state.get() != GameState::Splash;
    **visibility = if showing { Visibility::Visible } else { Visibility::Hidden };
    let Some(cursor) = touch.at.filter(|_| showing) else {
        return;
    };

    let ease = |rate: f32| 1.0 - (-rate * dt).exp();
    let press_rate = if touch.pressing { PRESS_ATTACK } else { PRESS_RELEASE };
    rig.press += (f32::from(touch.pressing) - rig.press) * ease(press_rate);
    rig.grab += (f32::from(touch.grabbing) - rig.grab) * ease(GRAB_EASE);

    let Ok(ray) = camera.viewport_to_world(camera_tf, cursor) else {
        return;
    };

    // From the eye rather than the ray's origin, which sits on the near plane. Park the
    // fingertip on the cursor by pulling the whole rig back along its own orientation.
    let rotation = cursor_rotation(camera_tf, rig.bank, rig.grab);
    let depth = HAND_DEPTH + rig.press * PRESS_DEPTH + rig.grab * PLANT_DEPTH;
    let target = camera_tf.translation() + *ray.direction * depth - rotation * fingertip(rig.press);

    let previous = transform.translation;
    // A planted palm is rigid with the glass it's holding: any glide left in the follow
    // reads as the hand skating over the thing it has hold of.
    let follow = ease(FOLLOW);
    let follow = follow + (1.0 - follow) * rig.grab;
    transform.translation = transform.translation.lerp(target, follow);

    // Bank into travel, the way anything moving through air does. Taken back into the
    // camera's frame first, so a leftward flick always leans the same way.
    let velocity = (transform.translation - previous) / dt;
    let local = camera_tf.rotation().inverse() * velocity;
    let target_bank = Vec2::new(
        (local.x * 0.10).clamp(-0.40, 0.40),
        (-local.y * 0.10).clamp(-0.35, 0.35),
    ) * (1.0 - rig.grab);
    let settle = (target_bank - rig.bank) * ease(SETTLE);
    rig.bank += settle;

    transform.rotation = rotation;
    transform.scale = Vec3::splat(HAND_SCALE);

    pose_fingers(rig, &mut joints, time.elapsed_secs());
}

/// Curl, splay and ripple. One pass over the joints.
fn pose_fingers(rig: &HandRig, joints: &mut Query<&mut Transform, Without<HandModel>>, t: f32) {
    let splay = rig.grab;
    for (index, [proximal, distal]) in rig.fingers.iter().enumerate() {
        // An idle ripple, so even a still hand breathes. Gone once it's holding on.
        let ripple = (t * 1.1 + index as f32 * 1.5).sin() * 0.09 * (1.0 - splay);

        // The index reaches out straight and presses; the others stay folded behind it,
        // which is what makes the gesture read as one finger rather than a paw.
        let (reach_proximal, reach_distal) = if index == 0 {
            (0.04 + rig.press * 0.42, 0.03 + rig.press * 0.5)
        } else {
            (1.02 + index as f32 * 0.05, 1.1)
        };

        // Knuckles nearly flat, tips bent in: a planted hand holds on with its fingertips,
        // not with a stiff paddle.
        let proximal_curl = (reach_proximal + ripple) * (1.0 - splay) + 0.16 * splay;
        let distal_curl = (reach_distal + ripple * 0.6) * (1.0 - splay) + 0.45 * splay;

        // Fanned about the middle finger. Wide on purpose: at these stubby lengths a
        // subtle fan reads as no fan at all. Positive yaw swings a -Z bone toward -X, so
        // the leftmost finger fans with the largest positive value.
        let fan = (1.5 - index as f32) * 0.30 * splay;

        if let Ok(mut joint) = joints.get_mut(*proximal) {
            joint.rotation = Quat::from_rotation_y(fan) * Quat::from_rotation_x(-proximal_curl);
        }
        if let Ok(mut joint) = joints.get_mut(*distal) {
            joint.rotation = Quat::from_rotation_x(-distal_curl);
        }
    }

    let [thumb_base, thumb_tip] = rig.thumb;
    if let Ok(mut joint) = joints.get_mut(thumb_base) {
        // The thumb spreads wide when the palm plants — the whole reach of the hand laid
        // against the glass.
        //
        // Barely curled the rest of the time, which is a departure from the game this came
        // from. Curling it toward the viewer foreshortens it into a stub when the hand is
        // seen face-on, and face-on is the only way this game ever sees it.
        joint.rotation = Quat::from_rotation_y(0.95 + 0.45 * splay)
            * Quat::from_rotation_x(-(0.30 * (1.0 - splay) + 0.08 * splay));
    }
    if let Ok(mut joint) = joints.get_mut(thumb_tip) {
        joint.rotation = Quat::from_rotation_x(-(0.42 * (1.0 - splay) + 0.32 * splay));
    }
}

/// Repaint the hand in whatever skin the player has chosen.
///
/// Here rather than in `settings`, because this module owns the materials — and driven by
/// watching the setting rather than by being told when it changes, so a tone restored from
/// disk at startup arrives by the same road as one just picked.
pub fn restyle_hand(
    settings: Res<crate::settings::Settings>,
    handles: Option<Res<HandMaterials>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(handles) = handles else {
        return;
    };
    if !settings.is_changed() && !handles.is_changed() {
        return;
    }

    let (skin, knuckle) = settings.skin();
    if let Some(mut material) = materials.get_mut(&handles.skin) {
        material.base_color = skin;
    }
    if let Some(mut material) = materials.get_mut(&handles.knuckle) {
        material.base_color = knuckle;
    }
}

/// The hand replaces the pointer, so the pointer goes away.
///
/// Only once the hand is actually built and visible. Hiding the system cursor and then
/// failing to draw a hand leaves the player with nothing to aim, which is a far worse bug
/// than a doubled cursor.
pub fn hide_the_pointer(
    hand: Query<&Visibility, With<HandModel>>,
    window: Single<&mut CursorOptions, With<PrimaryWindow>>,
) {
    let hand_showing = hand
        .iter()
        .any(|visibility| *visibility == Visibility::Visible);
    let mut cursor = window.into_inner();
    if cursor.visible == hand_showing {
        cursor.visible = !hand_showing;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fingertip has to stay put as the finger curls. It's the point the whole rig is
    /// positioned from, so if it wanders the hand slides off the thing it's pressing.
    #[test]
    fn the_fingertip_reaches_forward_and_draws_back_as_it_presses() {
        let open = fingertip(0.0);
        let pressed = fingertip(1.0);

        // Curling drops the tip and pulls it in toward the palm.
        assert!(pressed.y < open.y, "a pressing finger should drop its tip");
        assert!(pressed.z > open.z, "a pressing finger should draw its tip back");
        // And it stays on the index's own column, whatever it's doing.
        assert_eq!(open.x, pressed.x);
    }

    /// Both poses are reachable and distinct: a grab squares the hand up and takes the
    /// cursor's lean out, which is what makes a planted palm read as planted.
    #[test]
    fn a_grab_squares_the_hand_up() {
        let camera = GlobalTransform::default();
        let loose = cursor_rotation(&camera, Vec2::ZERO, 0.0);
        let planted = cursor_rotation(&camera, Vec2::ZERO, 1.0);

        assert!(
            loose.angle_between(planted) > 0.1,
            "the grab should visibly change the hand's attitude",
        );
    }
}
