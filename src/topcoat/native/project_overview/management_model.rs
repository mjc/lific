//! Frozen arguments for interrupted member/lead grants, independent of live pickers.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) enum Command {
    Add {
        user: i64,
        role: String,
    },
    Role {
        user: i64,
        role: String,
        previous: String,
    },
    Remove {
        user: i64,
    },
    Lead {
        user: Option<i64>,
        previous: Option<i64>,
    },
}
impl Command {
    pub(super) fn from_wire(
        kind: &str,
        user: Option<i64>,
        role: String,
        previous: String,
    ) -> Result<Self, crate::error::LificError> {
        let member =
            user.ok_or_else(|| crate::error::LificError::BadRequest("Choose a member.".into()));
        match kind {
            "member_add" => Ok(Self::Add {
                user: member?,
                role,
            }),
            "member_role" => Ok(Self::Role {
                user: member?,
                role,
                previous,
            }),
            "member_remove" => Ok(Self::Remove { user: member? }),
            "lead" => Ok(Self::Lead {
                user,
                previous: if previous.is_empty() {
                    None
                } else {
                    Some(previous.parse().map_err(|_| {
                        crate::error::LificError::BadRequest("Invalid previous lead.".into())
                    })?)
                },
            }),
            _ => Err(crate::error::LificError::BadRequest(
                "Unknown project management action.".into(),
            )),
        }
    }
    pub(super) fn wire(&self) -> (&'static str, Option<i64>, String, String) {
        match self {
            Self::Add { user, role } => ("member_add", Some(*user), role.clone(), String::new()),
            Self::Role {
                user,
                role,
                previous,
            } => ("member_role", Some(*user), role.clone(), previous.clone()),
            Self::Remove { user } => ("member_remove", Some(*user), String::new(), String::new()),
            Self::Lead { user, previous } => (
                "lead",
                *user,
                String::new(),
                previous.map(|id| id.to_string()).unwrap_or_default(),
            ),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) struct Continuation {
    pub(super) project: i64,
    pub(super) command: Command,
    pub(super) error: String,
    pub(super) automatic_note: String,
}
