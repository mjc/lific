mod app;
mod args;
mod setup;

pub(crate) use app::run;
pub use args::*;

pub mod agents_md;
pub mod bind;
pub mod connect;
pub mod credentials;
pub mod doctor;
pub mod exec;
pub mod git_hook;
pub mod http;
pub mod import;
pub mod instance;
pub mod key;
pub mod login;
pub mod mcp_instances;
pub mod mcp_proxy;
pub mod member;
pub mod output;
pub mod render;
pub mod service;
pub mod term;
#[cfg(test)]
pub(crate) mod test_env;
pub mod ui;
pub mod user;
pub mod weblinks;
