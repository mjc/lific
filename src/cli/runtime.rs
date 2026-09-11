//! Command execution policy derived from the parsed CLI command.
//!
//! Keeping these decisions together prevents the top-level runner from having
//! several independent command lists that can drift when a new subcommand is
//! added.

use super::{Command, ServiceAction};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommandKind {
    Data,
    Doctor,
    Completion,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CommandPlan {
    kind: CommandKind,
    requires_existing_database: bool,
    supports_http: bool,
    restores_sigpipe: bool,
}

impl CommandPlan {
    #[must_use]
    pub(crate) const fn is_data(self) -> bool {
        matches!(self.kind, CommandKind::Data)
    }

    #[must_use]
    pub(crate) const fn is_doctor(self) -> bool {
        matches!(self.kind, CommandKind::Doctor)
    }

    #[must_use]
    pub(crate) const fn is_completion(self) -> bool {
        matches!(self.kind, CommandKind::Completion)
    }

    #[must_use]
    pub(crate) const fn requires_existing_database(self) -> bool {
        self.requires_existing_database
    }

    #[must_use]
    pub(crate) const fn supports_http(self) -> bool {
        self.supports_http
    }

    #[must_use]
    pub(crate) const fn restores_sigpipe(self) -> bool {
        self.restores_sigpipe
    }
}

#[must_use]
pub(crate) fn plan(command: &Command) -> CommandPlan {
    let data = CommandPlan {
        kind: CommandKind::Data,
        requires_existing_database: true,
        supports_http: true,
        restores_sigpipe: true,
    };

    match command {
        Command::Issue { .. }
        | Command::Project { .. }
        | Command::Page { .. }
        | Command::Export { .. }
        | Command::Search { .. }
        | Command::Comment { .. }
        | Command::Module { .. }
        | Command::Label { .. }
        | Command::Folder { .. }
        | Command::Bind { .. }
        | Command::GitHook { .. } => data,
        Command::Doctor { .. } => CommandPlan {
            kind: CommandKind::Doctor,
            requires_existing_database: false,
            supports_http: false,
            restores_sigpipe: true,
        },
        Command::Completion { .. } => CommandPlan {
            kind: CommandKind::Completion,
            requires_existing_database: false,
            supports_http: false,
            restores_sigpipe: true,
        },
        Command::Init { .. }
        | Command::Login { .. }
        | Command::Logout { .. }
        | Command::Connect { .. }
        | Command::AgentsMd { .. }
        | Command::Import { .. } => no_database(),
        Command::Mcp {
            remote, instances, ..
        } => CommandPlan {
            kind: CommandKind::Other,
            requires_existing_database: !(*remote || instances.is_some()),
            supports_http: false,
            restores_sigpipe: false,
        },
        Command::Start {
            init_if_missing, ..
        } => CommandPlan {
            kind: CommandKind::Other,
            requires_existing_database: !init_if_missing,
            supports_http: false,
            restores_sigpipe: false,
        },
        Command::Service { action } => CommandPlan {
            kind: CommandKind::Other,
            requires_existing_database: matches!(action, ServiceAction::Install),
            supports_http: false,
            restores_sigpipe: true,
        },
        Command::ProjectArchive { .. }
        | Command::Dump { .. }
        | Command::Restore { .. }
        | Command::Instance { .. }
        | Command::Key { .. }
        | Command::User { .. }
        | Command::Member { .. } => existing_local(),
    }
}

const fn no_database() -> CommandPlan {
    CommandPlan {
        kind: CommandKind::Other,
        requires_existing_database: false,
        supports_http: false,
        restores_sigpipe: true,
    }
}

const fn existing_local() -> CommandPlan {
    CommandPlan {
        kind: CommandKind::Other,
        requires_existing_database: true,
        supports_http: false,
        restores_sigpipe: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_commands_share_local_and_http_policy() {
        let plan = plan(&Command::Bind {
            project: None,
            create: false,
        });
        assert!(plan.is_data());
        assert!(plan.requires_existing_database());
        assert!(plan.supports_http());
        assert!(plan.restores_sigpipe());
    }

    #[test]
    fn service_database_requirement_depends_on_action() {
        assert!(
            plan(&Command::Service {
                action: ServiceAction::Install,
            })
            .requires_existing_database()
        );
        assert!(
            !plan(&Command::Service {
                action: ServiceAction::Status,
            })
            .requires_existing_database()
        );
    }

    #[test]
    fn remote_mcp_and_first_boot_skip_database_guard() {
        assert!(
            !plan(&Command::Mcp {
                remote: true,
                url: None,
                instances: None,
            })
            .requires_existing_database()
        );
        assert!(
            !plan(&Command::Start {
                port: None,
                host: None,
                init_if_missing: true,
            })
            .requires_existing_database()
        );
    }
}
