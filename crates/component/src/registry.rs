use crate::{ComponentHandle, ComponentLink, StorefrontHandle};
use artwork::ArtworkService;
use dashmap::{DashMap, Entry};
use domain::{
    component::{Component, ComponentStorage},
    game::GameRepository,
};
use event::Topic;
use runtime::TokioHandle;
use std::sync::Arc;
use thiserror::Error;
use tracing::debug;

#[derive(Clone)]
pub struct ComponentRegistry {
    components: Arc<DashMap<String, ComponentHandle>>,
    context: RegistryContext,
}

impl ComponentRegistry {
    pub fn new(context: RegistryContext) -> Self {
        Self {
            components: Arc::default(),
            context,
        }
    }

    pub async fn register<C, E>(
        &self,
        build: impl AsyncFnOnce(ComponentLink) -> Result<C, E>,
    ) -> Result<ComponentHandle, E>
    where
        C: Component + 'static,
        E: From<RegisterError>,
    {
        let link = ComponentLink::new();
        let component: Arc<dyn Component> = Arc::new(build(link.clone()).await?);

        let id = component.metadata().id.to_owned();
        match self.components.entry(id) {
            Entry::Occupied(entry) => Err(RegisterError::Duplicate {
                id: entry.key().clone(),
            }
            .into()),
            Entry::Vacant(entry) => {
                let handle = ComponentHandle::new(component, self.context.clone());
                link.bind(&handle);
                entry.insert(handle.clone());
                debug!(component = handle.id(), "component registered");
                event::emit(Topic::ComponentRegistered);
                Ok(handle)
            }
        }
    }

    pub fn component(&self, id: &str) -> Option<ComponentHandle> {
        self.components.get(id).map(|c| c.value().clone())
    }

    pub fn components(&self) -> Vec<ComponentHandle> {
        self.components.iter().map(|c| c.value().clone()).collect()
    }

    pub fn storefront(&self, id: &str) -> Option<StorefrontHandle> {
        self.components.get(id).and_then(|c| c.value().storefront())
    }

    pub fn storefronts(&self) -> Vec<StorefrontHandle> {
        self.components
            .iter()
            .filter_map(|c| c.value().storefront())
            .collect()
    }
}

#[derive(Clone)]
pub struct RegistryContext {
    pub tokio_handle: TokioHandle,
    pub component_storage: Arc<dyn ComponentStorage>,
    pub game_repository: Arc<dyn GameRepository>,
    pub artwork_service: ArtworkService,
}

#[derive(Debug, Error)]
pub enum RegisterError {
    #[error("component `{id}` is already registered")]
    Duplicate { id: String },
}
