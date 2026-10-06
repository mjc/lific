#[cfg(test)]
mod admission_contract;
#[cfg(test)]
mod assembled;
pub(crate) mod board;
pub(crate) mod bookmark;
pub(crate) mod browser_inputs;
pub(crate) mod context;
pub(crate) mod deferred_delete;
pub(crate) mod handler_asset;
pub(crate) mod home;
pub(crate) mod home_activity;
pub(crate) mod home_activity_rate;
#[cfg(test)]
mod home_browser_edges;
pub(crate) mod home_data;
#[cfg(test)]
pub(crate) mod home_fixture;
pub(crate) mod home_live;
#[cfg(test)]
mod home_live_production;
pub(crate) mod home_local;
pub(crate) mod home_model;
#[cfg(test)]
mod home_production;
mod home_refresh;
pub(crate) mod home_sections;
pub(crate) mod home_shell;
#[cfg(test)]
mod home_shell_production;
pub(crate) mod home_view;
pub(crate) mod icons;
pub(crate) mod issue_edit;
#[cfg(test)]
mod limit_contract;
pub(crate) mod markdown;
#[cfg(test)]
mod markdown_edit;
pub(crate) mod palette_reference;
#[cfg(test)]
mod palette_reference_production;
#[cfg(test)]
pub(crate) mod probe;
pub(crate) mod public_route;
#[cfg(test)]
mod raw_lifecycle;
pub(crate) mod session;
#[cfg(test)]
mod session_idle;
#[cfg(test)]
mod session_redirect_production;
pub(crate) mod socket_admission;
#[cfg(test)]
mod stylesheet;
pub(crate) mod transport;

#[cfg(test)]
mod workspace_production;

pub(crate) mod issue_list;
pub(crate) mod workspace;

#[cfg(test)]
mod workspace_delete_production;

pub(crate) mod project_overview;

pub(crate) mod project_create;

pub(crate) mod project_sidebar;

mod motion;

#[cfg(test)]
mod common_owner_production;
