mod actor;
mod api;
mod auth;
mod authz;
#[cfg(test)]
mod authz_coverage_tests;
mod backup;
mod cli;
mod config;
mod db;
mod dump;
mod error;
mod export;
mod first_boot;
mod import;
mod issue_refs;
mod links;
mod mcp;
mod oauth;
mod preview;
mod project_archive;
mod ratelimit;
mod realtime;
mod repo_identity;
mod resolve_caller;
mod retention;
mod server;
mod storage;
#[cfg(test)]
mod test_env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    cli::run()
}
