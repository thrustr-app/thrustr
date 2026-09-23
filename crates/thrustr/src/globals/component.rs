use crate::adapters::ImageExt;
use artwork::ArtworkService;
use component::{ComponentHandle, ComponentRegistry, RegistryContext, StorefrontHandle};
use domain::{component::ComponentStorage, game::GameRepository};
use gpui::{App, Global, Image};
use runtime::TokioHandle;
use std::{collections::HashMap, sync::Arc};

pub(super) struct ComponentRegistryGlobal(ComponentRegistry);

impl Global for ComponentRegistryGlobal {}

#[derive(Default)]
struct ComponentIcons(HashMap<String, Option<Arc<Image>>>);

impl Global for ComponentIcons {}

pub(super) fn init(
    cx: &mut App,
    tokio_handle: TokioHandle,
    component_storage: Arc<dyn ComponentStorage>,
    game_repository: Arc<dyn GameRepository>,
    artwork_service: ArtworkService,
) -> ComponentRegistry {
    let registry = ComponentRegistry::new(RegistryContext {
        tokio_handle,
        component_storage,
        game_repository,
        artwork_service,
    });
    cx.set_global(ComponentRegistryGlobal(registry.clone()));
    cx.set_global(ComponentIcons::default());
    registry
}

pub trait ComponentRegistryExt {
    fn component_registry(&self) -> ComponentRegistry;

    fn component(&self, id: &str) -> Option<ComponentHandle> {
        self.component_registry().component(id)
    }

    fn storefronts(&self) -> Vec<StorefrontHandle> {
        self.component_registry().storefronts()
    }

    fn component_icon(&mut self, id: &str) -> Option<Arc<Image>>;
}

impl ComponentRegistryExt for App {
    fn component_registry(&self) -> ComponentRegistry {
        self.global::<ComponentRegistryGlobal>().0.clone()
    }

    fn component_icon(&mut self, id: &str) -> Option<Arc<Image>> {
        if let Some(icon) = self.global::<ComponentIcons>().0.get(id) {
            return icon.clone();
        }

        let icon = self.component(id)?.metadata().icon.map(|i| i.to_gpui());
        self.global_mut::<ComponentIcons>()
            .0
            .insert(id.to_owned(), icon.clone());
        icon
    }
}
