//! Native issue editing proof. Shared services own authorization and writes.

pub(crate) mod actions;
pub(crate) mod activity;
#[cfg(test)]
mod activity_production;
#[cfg(test)]
mod browser_production;
pub(crate) mod controls;
pub(crate) mod delete;
pub(crate) mod delete_menu;
pub(crate) mod export;
pub(crate) mod labels;
pub(crate) mod list_return;
mod model;
pub(crate) mod module_assignment;
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

#[cfg(test)]
mod decoration_production;

#[cfg(test)]
mod menu_production;
