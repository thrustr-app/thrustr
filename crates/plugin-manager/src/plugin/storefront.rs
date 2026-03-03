use crate::plugin::Plugin;
use async_trait::async_trait;
use ports::capabilities::Storefront;

#[async_trait]
impl Storefront for Plugin {}
