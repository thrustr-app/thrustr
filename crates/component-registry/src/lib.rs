use dashmap::DashMap;
use domain::{
    component::{Component, ComponentStorage},
    game::GameRepository,
};
use std::sync::Arc;

mod handles;

pub use handles::*;

#[derive(Clone)]
pub struct ComponentRegistry {
    components: Arc<DashMap<String, Arc<dyn Component>>>,
    component_storage: Arc<dyn ComponentStorage>,
    game_storage: Arc<dyn GameRepository>,
}

impl ComponentRegistry {
    pub fn new(
        component_storage: Arc<dyn ComponentStorage>,
        game_storage: Arc<dyn GameRepository>,
    ) -> Self {
        Self {
            components: Arc::new(DashMap::new()),
            component_storage,
            game_storage,
        }
    }

    pub fn register(&self, component: Arc<dyn Component>) {
        self.components
            .insert(component.metadata().id.to_owned(), component);
    }

    pub fn component(&self, id: &str) -> Option<ComponentHandle> {
        self.components.get(id).map(|c| {
            ComponentHandle::new(
                Arc::clone(c.value()),
                Arc::clone(&self.component_storage),
                Arc::clone(&self.game_storage),
            )
        })
    }

    pub fn components(&self) -> Vec<ComponentHandle> {
        self.components
            .iter()
            .map(|c| {
                ComponentHandle::new(
                    Arc::clone(c.value()),
                    Arc::clone(&self.component_storage),
                    Arc::clone(&self.game_storage),
                )
            })
            .collect()
    }

    pub fn plugins(&self) -> Vec<ComponentHandle> {
        self.components
            .iter()
            .filter(|c| c.value().metadata().origin.is_plugin())
            .map(|c| {
                ComponentHandle::new(
                    Arc::clone(c.value()),
                    Arc::clone(&self.component_storage),
                    Arc::clone(&self.game_storage),
                )
            })
            .collect()
    }

    pub fn storefronts(&self) -> Vec<StorefrontHandle> {
        self.components
            .iter()
            .filter_map(|c| {
                Arc::clone(c.value()).storefront().map(|s| {
                    StorefrontHandle::new(
                        s,
                        Arc::clone(&self.component_storage),
                        Arc::clone(&self.game_storage),
                    )
                })
            })
            .collect()
    }

    pub fn storefront(&self, id: &str) -> Option<StorefrontHandle> {
        self.components
            .get(id)
            .and_then(|c| Arc::clone(c.value()).storefront())
            .map(|s| {
                StorefrontHandle::new(
                    s,
                    Arc::clone(&self.component_storage),
                    Arc::clone(&self.game_storage),
                )
            })
    }
}
