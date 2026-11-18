use crate::plugin::{PluginRuntime, guest_call};
use crate::wit::exports::thrustr::plugin::auth::{
    self, LoginCallback, LoginFlow as PluginLoginFlow, LoginForm as PluginLoginForm,
    LoginRequest as PluginLoginRequest,
};
use anyhow::ensure;
use async_trait::async_trait;
use domain::component::{Auth, Error, LoginFlow, LoginForm, LoginRequest};
use std::sync::Arc;

pub struct PluginAuth {
    runtime: Arc<PluginRuntime>,
    indices: auth::GuestIndices,
    form: Option<LoginForm>,
}

impl PluginAuth {
    pub fn resolve(
        runtime: &Arc<PluginRuntime>,
        form: Option<LoginForm>,
    ) -> anyhow::Result<Option<Self>> {
        let Some(indices) = runtime.export("auth", auth::GuestIndices::new)? else {
            ensure!(
                form.is_none(),
                "plugin manifest has `auth` but the plugin does not export Auth"
            );
            return Ok(None);
        };
        Ok(Some(Self {
            runtime: runtime.clone(),
            indices,
            form,
        }))
    }
}

#[async_trait]
impl Auth for PluginAuth {
    fn login_form(&self) -> Option<&LoginForm> {
        self.form.as_ref()
    }

    async fn login_flow(&self) -> Result<Option<LoginFlow>, Error> {
        let flow = guest_call!(self.runtime, self.indices, |auth, accessor| {
            auth.call_get_login_flow(accessor)
        })
        .await?;

        Ok(flow.map(Into::into))
    }

    async fn login(&self, request: LoginRequest) -> Result<(), Error> {
        guest_call!(self.runtime, self.indices, |auth, accessor| {
            auth.call_login(accessor, request.into())
        })
        .await
    }

    async fn logout(&self) -> Result<(), Error> {
        guest_call!(self.runtime, self.indices, |auth, accessor| {
            auth.call_logout(accessor)
        })
        .await
    }
}

impl From<PluginLoginFlow> for LoginFlow {
    fn from(value: PluginLoginFlow) -> Self {
        Self {
            url: value.url,
            target: value.target,
        }
    }
}

impl From<LoginRequest> for PluginLoginRequest {
    fn from(value: LoginRequest) -> Self {
        match value {
            LoginRequest::Flow { url, body } => Self::Flow(LoginCallback { url, body }),
            LoginRequest::Form { fields } => Self::Form(PluginLoginForm {
                fields: fields
                    .into_iter()
                    .map(|(id, value)| (id, value.into()))
                    .collect(),
            }),
        }
    }
}
