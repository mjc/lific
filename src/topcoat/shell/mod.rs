//! Route parsing shared by native application screens.

pub(crate) const STYLESHEET: &str = include_str!("assets/shell.css");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Layout {
    Auth,
    Private,
    Public,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Page<'a> {
    Login,
    Signup,
    Home,
    Settings,
    InstanceSettings,
    ProjectNew,
    ProjectImport,
    Overview,
    Issues,
    Board,
    Graph,
    IssueNew,
    IssueDetail(&'a str),
    Pages,
    Record(&'a str),
    Files,
    Modules,
    ModuleDetail(&'a str),
    Plans,
    PlanDetail(&'a str),
    Activity,
    Insights,
    NotFound,
}

impl<'a> Page<'a> {
    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::Login => "Log in",
            Self::Signup => "Sign up",
            Self::Home => "Home",
            Self::Settings => "Settings",
            Self::InstanceSettings => "Instance settings",
            Self::ProjectNew => "New project",
            Self::ProjectImport => "Import project",
            Self::Overview => "Overview",
            Self::Issues => "Issues",
            Self::Board => "Board",
            Self::Graph => "Graph",
            Self::IssueNew => "New issue",
            Self::IssueDetail(_) => "Issue detail",
            Self::Pages => "Pages",
            Self::Record(_) => "Page detail",
            Self::Files => "Files",
            Self::Modules => "Modules",
            Self::ModuleDetail(_) => "Module detail",
            Self::Plans => "Plans",
            Self::PlanDetail(_) => "Plan detail",
            Self::Activity => "Activity",
            Self::Insights => "Insights",
            Self::NotFound => "Page not found",
        }
    }

    pub(crate) fn resource(self) -> Option<&'a str> {
        match self {
            Self::IssueDetail(id)
            | Self::Record(id)
            | Self::ModuleDetail(id)
            | Self::PlanDetail(id) => Some(id),
            _ => None,
        }
    }

    fn public(self) -> bool {
        matches!(
            self,
            Self::Issues | Self::Board | Self::IssueDetail(_) | Self::Pages | Self::Record(_)
        )
    }

    pub(crate) fn navigation_page(self) -> Self {
        match self {
            Self::Board | Self::IssueNew | Self::IssueDetail(_) => Self::Issues,
            Self::Record(_) => Self::Pages,
            Self::ModuleDetail(_) => Self::Modules,
            Self::PlanDetail(_) => Self::Plans,
            _ => self,
        }
    }
}

/// The query and fragment are retained verbatim for later domain handlers.
/// Layout selection describes presentation, never authorization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedRoute<'a> {
    pub(crate) layout: Layout,
    pub(crate) page: Page<'a>,
    pub(crate) project: Option<&'a str>,
    pub(crate) query: &'a str,
    pub(crate) fragment: &'a str,
    /// Canonical destination for the two existing short public link forms.
    pub(crate) redirect: Option<String>,
}

impl<'a> ParsedRoute<'a> {
    /// Accepts direct paths and legacy browser hash routes. HTTP handlers
    /// receive only path/query; the browser owns restoration of hash routes.
    pub(crate) fn parse(input: &'a str) -> Self {
        let input = if let Some((_, hash)) = input.split_once('#')
            && hash.starts_with('/')
        {
            hash
        } else {
            input
        };
        let (input, fragment) = input.split_once('#').unwrap_or((input, ""));
        let (path, query) = input.split_once('?').unwrap_or((input, ""));
        let mut route = Self {
            layout: Layout::Private,
            page: Page::NotFound,
            project: None,
            query,
            fragment,
            redirect: None,
        };

        route.page = match path {
            "/login" => {
                route.layout = Layout::Auth;
                Page::Login
            }
            "/signup" => {
                route.layout = Layout::Auth;
                Page::Signup
            }
            "/" => Page::Home,
            "/settings" => Page::Settings,
            "/settings/instance" => Page::InstanceSettings,
            "/projects/new" => Page::ProjectNew,
            "/projects/import" => Page::ProjectImport,
            _ => return route.parse_project(path),
        };
        route
    }

    fn parse_project(mut self, path: &'a str) -> Self {
        let Some(path) = path.strip_prefix('/') else {
            return self;
        };
        let (public, path) = match path.strip_prefix("public/") {
            Some(path) => (true, path),
            None => (false, path),
        };
        let (project, rest) = path.split_once('/').unwrap_or((path, ""));
        if !project_identifier(project) {
            return self;
        }
        self.project = Some(project);
        if public {
            self.layout = Layout::Public;
            // Preserve published project and legacy issue deep links.
            let destination = if rest.is_empty() {
                Some(format!("/public/{project}/issues"))
            } else if issue_identifier(rest) {
                Some(format!("/public/{project}/issues/{rest}"))
            } else {
                None
            };
            if let Some(mut destination) = destination {
                if !self.query.is_empty() {
                    destination.push('?');
                    destination.push_str(self.query);
                }
                if !self.fragment.is_empty() {
                    destination.push('#');
                    destination.push_str(self.fragment);
                }
                self.redirect = Some(destination);
                return self;
            }
        }

        let (section, target) = rest.split_once('/').unwrap_or((rest, ""));
        let section = section.to_ascii_lowercase();
        self.page = match (section.as_str(), target) {
            ("overview" | "settings", "") if !rest.ends_with('/') => Page::Overview,
            ("issues", "") if !rest.ends_with('/') => Page::Issues,
            ("board", "") if !rest.ends_with('/') => Page::Board,
            ("graph", "") if !rest.ends_with('/') => Page::Graph,
            ("issues", value) if value.eq_ignore_ascii_case("new") => Page::IssueNew,
            ("issues", id) if issue_identifier(id) => Page::IssueDetail(id),
            ("pages", "") if !rest.ends_with('/') => Page::Pages,
            ("pages", id) if digits(id) && id.parse::<i64>().is_ok() => Page::Record(id),
            ("files", "") if !rest.ends_with('/') => Page::Files,
            ("modules", "") if !rest.ends_with('/') => Page::Modules,
            ("modules", id) if digits(id) && id.parse::<i64>().is_ok() => Page::ModuleDetail(id),
            ("plans", "") if !rest.ends_with('/') => Page::Plans,
            ("plans", id) if digits(id) && id.parse::<i64>().is_ok() => Page::PlanDetail(id),
            ("activity", "") if !rest.ends_with('/') => Page::Activity,
            ("insights", "") if !rest.ends_with('/') => Page::Insights,
            _ => Page::NotFound,
        };
        if public && !self.page.public() {
            self.page = Page::NotFound;
        }
        self
    }
}

fn project_identifier(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes.next().is_some_and(|byte| byte.is_ascii_alphabetic())
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn digits(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn issue_identifier(value: &str) -> bool {
    value
        .rsplit_once('-')
        .is_some_and(|(project, number)| project_identifier(project) && digits(number))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_route_parity_preserves_each_private_family() {
        let cases = [
            ("/login", Layout::Auth, Page::Login),
            ("/signup", Layout::Auth, Page::Signup),
            ("/", Layout::Private, Page::Home),
            ("/settings", Layout::Private, Page::Settings),
            (
                "/settings/instance",
                Layout::Private,
                Page::InstanceSettings,
            ),
            ("/projects/new", Layout::Private, Page::ProjectNew),
            ("/projects/import", Layout::Private, Page::ProjectImport),
            ("/LIF/overview", Layout::Private, Page::Overview),
            ("/LIF/settings", Layout::Private, Page::Overview),
            ("/LIF/issues", Layout::Private, Page::Issues),
            ("/LIF/board", Layout::Private, Page::Board),
            ("/LIF/graph", Layout::Private, Page::Graph),
            ("/LIF/issues/new", Layout::Private, Page::IssueNew),
            (
                "/LIF/issues/LIF-205",
                Layout::Private,
                Page::IssueDetail("LIF-205"),
            ),
            ("/LIF/pages", Layout::Private, Page::Pages),
            ("/LIF/pages/7", Layout::Private, Page::Record("7")),
            ("/LIF/files", Layout::Private, Page::Files),
            ("/LIF/modules", Layout::Private, Page::Modules),
            ("/LIF/modules/8", Layout::Private, Page::ModuleDetail("8")),
            ("/LIF/plans", Layout::Private, Page::Plans),
            ("/LIF/plans/9", Layout::Private, Page::PlanDetail("9")),
            ("/LIF/activity", Layout::Private, Page::Activity),
            ("/LIF/insights", Layout::Private, Page::Insights),
        ];
        for (path, layout, page) in cases {
            let route = ParsedRoute::parse(path);
            assert_eq!((route.layout, route.page), (layout, page), "{path}");
            assert!(route.redirect.is_none(), "{path}");
        }
        assert_eq!(
            ParsedRoute::parse("/lif/ISSUES/Lif-1").page,
            Page::IssueDetail("Lif-1")
        );
    }

    #[test]
    fn shell_public_routes_limit_destinations_and_keep_aliases() {
        for path in [
            "/public/LIF/issues",
            "/public/LIF/board",
            "/public/LIF/issues/LIF-42",
            "/public/LIF/pages",
            "/public/LIF/pages/7",
        ] {
            let route = ParsedRoute::parse(path);
            assert_eq!(route.layout, Layout::Public, "{path}");
            assert!(route.page.public(), "{path}");
        }
        for suffix in [
            "overview",
            "settings",
            "graph",
            "files",
            "modules",
            "plans",
            "activity",
            "insights",
            "issues/new",
        ] {
            let path = format!("/public/LIF/{suffix}");
            let route = ParsedRoute::parse(&path);
            assert_eq!(route.layout, Layout::Public, "{path}");
            assert_eq!(route.page, Page::NotFound, "{path}");
        }
        for path in ["/public/LIF", "/public/LIF/"] {
            assert_eq!(
                ParsedRoute::parse(path).redirect.as_deref(),
                Some("/public/LIF/issues")
            );
        }
        assert_eq!(
            ParsedRoute::parse("/public/LIF/LIF-42?view=all#comment-123")
                .redirect
                .as_deref(),
            Some("/public/LIF/issues/LIF-42?view=all#comment-123")
        );
    }

    #[test]
    fn shell_deep_links_keep_query_fragment_and_reject_malformed_paths() {
        let route = ParsedRoute::parse("/#/LIF/issues/new?module=7&status=todo#comment-123");
        assert_eq!(route.page, Page::IssueNew);
        assert_eq!(route.project, Some("LIF"));
        assert_eq!(route.query, "module=7&status=todo");
        assert_eq!(route.fragment, "comment-123");
        for path in [
            "/LIF/issues/",
            "/LIF/issues/LIF-no",
            "/LIF/pages/-1",
            "/LIF/pages/9223372036854775808",
            "/LIF/modules/9223372036854775808",
            "/LIF/plans/9223372036854775808",
            "/LIF/pages/7/extra",
            "/1LIF/issues",
            "/LIF//issues",
            "/unknown",
        ] {
            assert_eq!(ParsedRoute::parse(path).page, Page::NotFound, "{path}");
        }
    }
}
