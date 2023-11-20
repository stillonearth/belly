use crate::element::Element;
use crate::eml::WidgetData;
use crate::eml::{parse, Param, Slots};
use crate::ess::{PropertyExtractor, PropertyTransformer};
use anyhow::Error;
use bevy::reflect::TypePath;
use bevy::{
    asset::{AssetLoader, LoadedAsset},
    prelude::*,
    reflect::TypeUuid,
    utils::HashMap,
};
use std::sync::Arc;
use tagstr::*;

use super::build::WidgetRegistry;

pub enum EmlNode {
    Element(EmlElement),
    Text(String),
    Slot(Tag, Vec<EmlNode>),
}

#[derive(Default)]
pub struct EmlElement {
    pub(crate) name: Tag,
    pub(crate) params: HashMap<String, String>,
    pub(crate) children: Vec<EmlNode>,
}

impl EmlElement {
    pub fn new(name: Tag) -> EmlElement {
        EmlElement { name, ..default() }
    }
}

#[derive(Component)]
pub struct EmlScene {
    asset: Handle<EmlAsset>,
}

impl EmlScene {
    pub fn new(asset: Handle<EmlAsset>) -> EmlScene {
        EmlScene { asset }
    }
}

#[derive(TypeUuid, Clone, TypePath, Asset)]
#[uuid = "f8d22a65-d671-4fa6-ae8f-0dccdb387ddd"]
pub struct EmlAsset {
    root: Arc<EmlNode>,
}

impl EmlAsset {
    pub fn write(&self, world: &mut World, parent: Entity) {
        // let node = E
        walk(&self.root, world, Some(parent));
    }
}

fn walk(node: &EmlNode, world: &mut World, parent: Option<Entity>) -> Option<Entity> {
    match node {
        EmlNode::Text(text) => {
            let entity = world
                .spawn(TextBundle {
                    text: Text::from_section(text, Default::default()),
                    ..default()
                })
                .insert(Element::inline())
                .id();
            Some(entity)
        }
        EmlNode::Slot(name, elements) => {
            let slots = world.resource::<Slots>().clone();
            let entities: Vec<Entity> = elements
                .iter()
                .filter_map(|e| walk(e, world, None))
                .collect();
            slots.insert(*name, entities);
            None
        }
        EmlNode::Element(elem) => {
            let Some(builder) = world.resource::<WidgetRegistry>().get(elem.name) else {
                error!("Invalid tag name: {}", elem.name.as_str());
                return None;
            };
            let entity = parent.unwrap_or_else(|| world.spawn_empty().id());
            let mut data = WidgetData::new(entity);
            for (name, value) in elem.params.iter() {
                let attr = Param::new(name, value.clone().into());
                data.params.add(attr);
            }
            for child in elem.children.iter() {
                if let Some(entity) = walk(child, world, None) {
                    data.children.push(entity);
                }
            }
            builder.build(world, data);
            Some(entity)
        }
    }
}

#[derive(Default)]
pub(crate) struct EmlLoader {
    pub(crate) registry: WidgetRegistry,
    pub(crate) transformer: PropertyTransformer,
    pub(crate) extractor: PropertyExtractor,
}

impl AssetLoader for EmlLoader {
    fn extensions(&self) -> &[&str] {
        &["eml"]
    }

    fn load<'a>(
        &'a self,
        bytes: &'a [u8],
        settings: &'a Self::Settings,
        load_context: &'a mut bevy::asset::LoadContext,
    ) -> bevy::utils::BoxedFuture<'a, Result<(), Error>> {
        Box::pin(async move {
            let source = std::str::from_utf8(bytes)?;

            match parse::parse(source, self) {
                Ok(root) => {
                    let asset = EmlAsset {
                        root: Arc::new(root),
                    };
                    load_context.add_labeled_asset("default".to_string(), LoadedAsset::from(asset));
                    Ok(())
                }
                Err(err) => {
                    let path = load_context.path();
                    error!("Error parsing {}:\n\n{}", path.to_str().unwrap(), err);
                    Err(Error::new(err)
                        .context(format!("Unable to parse {}", path.to_str().unwrap())))
                }
            }
        })
    }
}

pub fn update_eml_scene(
    scenes: Query<(Entity, &EmlScene, Option<&Children>)>,
    mut events: EventReader<AssetEvent<EmlAsset>>,
    assets: Res<Assets<EmlAsset>>,
    mut commands: Commands,
) {
    for event in events.read() {
        if let AssetEvent::Added { id } = event {
            let asset = assets.get(id.into()).unwrap();
            for (entity, _, _) in scenes.iter().filter(|(_, s, _)| &s.asset == id.into()) {
                let asset = asset.clone();
                commands.add(move |world: &mut World| {
                    asset.write(world, entity);
                });
            }
        } else if let AssetEvent::Modified { id } = event {
            let asset = assets.get(id.into()).unwrap();
            for (entity, _, children) in scenes.iter().filter(|(_, s, _)| &s.asset == id.into()) {
                if let Some(children) = children {
                    for ch in children.iter() {
                        commands.entity(*ch).despawn_recursive();
                    }
                }
                let asset = asset.clone();
                commands.add(move |world: &mut World| {
                    asset.write(world, entity);
                });
            }
        }
    }
}
