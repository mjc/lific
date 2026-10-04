//! Native issue editing proof. Shared services own authorization and writes.

pub(crate) mod actions;
#[cfg(test)]
mod browser_production;
pub(crate) mod controls;
mod model;
pub(crate) mod view;
