use crate::plugin::PluginState;
use crate::wit::{export_name, thrustr::plugin::types::Error as PluginError};
use component::Timers;
use domain::component::{ComponentStorage, Error as ComponentError};
use reqwest::Client;
use runtime::TokioHandle;
use std::sync::Arc;
use wasmtime::component::{Instance, InstancePre};
use wasmtime::{Engine, ResourceLimiter, Store};

type CallResult<R> = wasmtime::Result<Result<R, PluginError>>;

const MAX_FUEL: u64 = 10_000_000_000;
const FUEL_YIELD_INTERVAL: u64 = 10_000_000;

pub struct PluginRuntime {
    pub id: String,
    pub allowed_hosts: Arc<[String]>,
    pub engine: Engine,
    pub pre: InstancePre<PluginState>,
    pub storage: Arc<dyn ComponentStorage>,
    pub tokio_handle: TokioHandle,
    pub http_client: Client,
    pub timers: Timers,
}

impl PluginRuntime {
    /// `None` if the plugin doesn't export `interface`; an error if it does
    /// but it doesn't match the host's bindings.
    pub fn export<T>(
        &self,
        interface: &str,
        new: impl FnOnce(&InstancePre<PluginState>) -> wasmtime::Result<T>,
    ) -> anyhow::Result<Option<T>> {
        let name = export_name(interface);
        if self.pre.component().get_export_index(None, &name).is_none() {
            return Ok(None);
        }
        let indices = new(&self.pre).map_err(|e| {
            e.context(format!(
                "plugin exports `{name}` but it does not match the host; rebuild it against the current pdk"
            ))
        })?;
        Ok(Some(indices))
    }

    pub async fn call<R, F, Fut>(self: &Arc<Self>, f: F) -> Result<R, ComponentError>
    where
        R: Send + 'static,
        F: FnOnce(Instance, Store<PluginState>) -> Fut + Send + 'static,
        Fut: Future<Output = CallResult<R>> + Send + 'static,
    {
        let runtime = Arc::clone(self);

        self.tokio_handle
            .spawn(async move {
                let state = PluginState::new(
                    &runtime.id,
                    runtime.storage.clone(),
                    runtime.http_client.clone(),
                    runtime.allowed_hosts.clone(),
                    runtime.timers.clone(),
                );
                let mut store = Store::new(&runtime.engine, state);

                store.limiter(|state| state.limits() as &mut dyn ResourceLimiter);

                store
                    .set_fuel(MAX_FUEL)
                    .and_then(|()| store.fuel_async_yield_interval(Some(FUEL_YIELD_INTERVAL)))
                    .map_err(|e| ComponentError::Other(format!("Fuel setup failed: {e}")))?;

                let instance = runtime
                    .pre
                    .instantiate_async(&mut store)
                    .await
                    .map_err(|e| ComponentError::Other(format!("Instantiation failed: {e}")))?;

                f(instance, store)
                    .await
                    .map_err(|e| ComponentError::Other(format!("Wasm call failed: {e}")))?
                    .map_err(ComponentError::from)
            })
            .await
            .map_err(|e| ComponentError::Other(format!("Plugin task failed: {e}")))?
    }
}

/// Instantiates the plugin, loads the export behind `indices` and runs one
/// guest call on it.
macro_rules! guest_call {
    ($runtime:expr, $indices:expr, |$guest:ident, $accessor:ident| $call:expr) => {{
        let indices = $indices.clone();
        $runtime
            .call(|instance, mut store| async move {
                let $guest = indices.load(&mut store, &instance)?;
                store
                    .run_concurrent(async |$accessor| $call.await)
                    .await
                    .and_then(|result| result)
            })
            .await
    }};
}
pub(crate) use guest_call;
