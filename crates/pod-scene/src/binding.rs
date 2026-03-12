use crate::prefab::PrefabComponent;
use hecs::Entity;
use pod_core::{
    Camera3D, Collider, ColorRect, FlyCameraController, FollowCameraController, Health, Label,
    Light, Material, Mesh, Movement, OrbitCameraController, Parent3D, Perception, RigidBody,
    Script, Sprite, Transform, Transform3D, Velocity,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::collections::HashMap;

pub trait NativeComponentBinding: Serialize + DeserializeOwned + Clone {
    const COMPONENT_NAME: &'static str;

    fn to_component_value(&self) -> Result<serde_json::Value, String> {
        serde_json::to_value(self).map_err(|err| err.to_string())
    }

    fn from_component_value(value: &serde_json::Value) -> Result<Self, String> {
        serde_json::from_value(value.clone()).map_err(|err| err.to_string())
    }
}

macro_rules! impl_native_binding {
    ($type:ty, $name:literal) => {
        impl NativeComponentBinding for $type {
            const COMPONENT_NAME: &'static str = $name;
        }
    };
}

impl_native_binding!(Transform, "Transform");
impl_native_binding!(Transform3D, "Transform3D");
impl_native_binding!(Velocity, "Velocity");
impl_native_binding!(RigidBody, "RigidBody");
impl_native_binding!(Collider, "Collider");
impl_native_binding!(Health, "Health");
impl_native_binding!(Sprite, "Sprite");
impl_native_binding!(ColorRect, "ColorRect");
impl_native_binding!(Label, "Label");
impl_native_binding!(Perception, "Perception");
impl_native_binding!(Script, "Script");
impl_native_binding!(Movement, "Movement");
impl_native_binding!(Parent3D, "Parent3D");
impl_native_binding!(Mesh, "Mesh");
impl_native_binding!(Material, "Material");
impl_native_binding!(Camera3D, "Camera3D");
impl_native_binding!(Light, "Light");
impl_native_binding!(OrbitCameraController, "OrbitCameraController");
impl_native_binding!(FlyCameraController, "FlyCameraController");
impl_native_binding!(FollowCameraController, "FollowCameraController");

#[derive(Debug, Clone)]
pub enum NativeComponent {
    Transform(Transform),
    Transform3D(Transform3D),
    Velocity(Velocity),
    RigidBody(RigidBody),
    Collider(Collider),
    Health(Health),
    Sprite(Sprite),
    ColorRect(ColorRect),
    Label(Label),
    Perception(Perception),
    Script(Script),
    Movement(Movement),
    Parent3D(Parent3D),
    Mesh(Mesh),
    Material(Material),
    Camera3D(Camera3D),
    Light(Light),
    OrbitCameraController(OrbitCameraController),
    FlyCameraController(FlyCameraController),
    FollowCameraController(FollowCameraController),
}

impl NativeComponent {
    pub fn insert_into_world(&self, ecs: &mut hecs::World, entity: Entity) -> Result<(), String> {
        match self {
            Self::Transform(value) => ecs.insert_one(entity, *value),
            Self::Transform3D(value) => ecs.insert_one(entity, *value),
            Self::Velocity(value) => ecs.insert_one(entity, *value),
            Self::RigidBody(value) => ecs.insert_one(entity, *value),
            Self::Collider(value) => ecs.insert_one(entity, *value),
            Self::Health(value) => ecs.insert_one(entity, *value),
            Self::Sprite(value) => ecs.insert_one(entity, value.clone()),
            Self::ColorRect(value) => ecs.insert_one(entity, *value),
            Self::Label(value) => ecs.insert_one(entity, value.clone()),
            Self::Perception(value) => ecs.insert_one(entity, *value),
            Self::Script(value) => ecs.insert_one(entity, value.clone()),
            Self::Movement(value) => ecs.insert_one(entity, *value),
            Self::Parent3D(value) => ecs.insert_one(entity, *value),
            Self::Mesh(value) => ecs.insert_one(entity, value.clone()),
            Self::Material(value) => ecs.insert_one(entity, value.clone()),
            Self::Camera3D(value) => ecs.insert_one(entity, value.clone()),
            Self::Light(value) => ecs.insert_one(entity, value.clone()),
            Self::OrbitCameraController(value) => ecs.insert_one(entity, *value),
            Self::FlyCameraController(value) => ecs.insert_one(entity, *value),
            Self::FollowCameraController(value) => ecs.insert_one(entity, *value),
        }
        .map_err(|err| err.to_string())
    }
}

macro_rules! match_native_component {
    ($name:expr, $value:expr, $([$component_name:literal, $variant:ident, $type:ty]),+ $(,)?) => {
        match $name {
            $(
                $component_name => Ok(Some(NativeComponent::$variant(
                    <$type as NativeComponentBinding>::from_component_value($value)?,
                ))),
            )+
            _ => Ok(None),
        }
    };
}

pub fn parse_native_component(
    name: &str,
    value: &serde_json::Value,
) -> Result<Option<NativeComponent>, String> {
    match_native_component!(
        name,
        value,
        ["Transform", Transform, Transform],
        ["Transform3D", Transform3D, Transform3D],
        ["Velocity", Velocity, Velocity],
        ["RigidBody", RigidBody, RigidBody],
        ["Collider", Collider, Collider],
        ["Health", Health, Health],
        ["Sprite", Sprite, Sprite],
        ["ColorRect", ColorRect, ColorRect],
        ["Label", Label, Label],
        ["Perception", Perception, Perception],
        ["Script", Script, Script],
        ["Movement", Movement, Movement],
        ["Parent3D", Parent3D, Parent3D],
        ["Mesh", Mesh, Mesh],
        ["Material", Material, Material],
        ["Camera3D", Camera3D, Camera3D],
        ["Light", Light, Light],
        [
            "OrbitCameraController",
            OrbitCameraController,
            OrbitCameraController
        ],
        [
            "FlyCameraController",
            FlyCameraController,
            FlyCameraController
        ],
        [
            "FollowCameraController",
            FollowCameraController,
            FollowCameraController
        ],
    )
}

pub fn insert_bound_components(
    components: &HashMap<String, PrefabComponent>,
    ecs: &mut hecs::World,
    entity: Entity,
) -> Result<Vec<String>, String> {
    let mut component_names: Vec<&str> = components.keys().map(String::as_str).collect();
    component_names.sort_unstable();

    let mut ignored = Vec::new();

    for component_name in component_names {
        let component = components
            .get(component_name)
            .expect("component name came from map keys");
        match parse_native_component(component_name, component.as_json())? {
            Some(native_component) => native_component.insert_into_world(ecs, entity)?,
            None => ignored.push(component_name.to_string()),
        }
    }

    Ok(ignored)
}
