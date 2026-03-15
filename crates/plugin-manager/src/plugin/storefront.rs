use crate::plugin::Plugin;
use async_trait::async_trait;
use domain::capabilities::Storefront;

#[async_trait]
impl Storefront for Plugin {}
