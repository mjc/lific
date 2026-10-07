pub(crate) mod activity_text;
#[cfg(test)]
mod admission_contract;
#[cfg(test)]
mod assembled;
mod auth_actions;
mod auth_form;
mod auth_shell;
pub(crate) mod avatar;
pub(crate) mod board;
pub(crate) mod bookmark;
pub(crate) mod browser_inputs;
pub(crate) mod context;
pub(crate) mod dates;
pub(crate) mod deferred_delete;
pub(crate) mod dependency_graph;
pub(crate) mod error_state;
pub(crate) mod files;
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
pub(crate) mod insights;
pub(crate) mod issue_create;
pub(crate) mod issue_edit;
#[cfg(test)]
mod limit_contract;
pub(crate) mod login;
pub(crate) mod markdown;
#[cfg(test)]
mod markdown_edit;
pub(crate) mod mascot;
pub(crate) mod modules;
pub(crate) mod numbers;
pub(crate) mod pages;
pub(crate) mod palette_reference;
#[cfg(test)]
mod palette_reference_production;
pub(crate) mod plans;
pub(crate) mod preloads;
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
pub(crate) mod signup;
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

pub(crate) mod project_authority;
pub(crate) mod project_overview;

pub(crate) mod project_create;

pub(crate) mod project_activity;
pub(crate) mod project_sidebar;

mod motion;
pub(crate) mod navigation;

#[cfg(test)]
mod common_owner_production;

#[cfg(test)]
mod knowledge_production;

#[cfg(test)]
mod creation_modules_production;

#[cfg(test)]
mod navigation_authority_tests;
