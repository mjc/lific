//! Native issue editing proof. Shared services own authorization and writes.

pub(crate) mod actions;
#[cfg(test)]
mod browser_production;
pub(crate) mod controls;
pub(crate) mod export;
mod model;
pub(crate) mod route;
pub(crate) mod view;

#[cfg(test)]
mod markdown_production;
#[cfg(test)]
mod production;

#[cfg(test)]
mod detail_production;

#[cfg(test)]
mod export_production;
