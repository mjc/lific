//! Semantic UI roles mapped to the original approved Lucide geometry.

#[derive(Clone, Copy, Debug, PartialEq, Eq, strum::IntoStaticStr, strum::VariantArray)]
pub(crate) enum UiIcon {
    #[strum(serialize = "AlertTriangle")]
    Warning,
    #[strum(serialize = "ArrowDown")]
    MoveDown,
    #[strum(serialize = "ArrowLeft")]
    Back,
    #[strum(serialize = "ArrowRight")]
    Forward,
    #[strum(serialize = "ArrowUp")]
    MoveUp,
    #[strum(serialize = "ArrowUpRight")]
    RecentActivity,
    #[strum(serialize = "Box")]
    Module,
    #[strum(serialize = "Check")]
    Selected,
    #[strum(serialize = "ChevronDown")]
    Expand,
    #[strum(serialize = "ChevronLeft")]
    Previous,
    #[strum(serialize = "ChevronRight")]
    Next,
    #[strum(serialize = "Circle")]
    Issue,
    #[strum(serialize = "CircleAlert")]
    Error,
    #[strum(serialize = "CircleCheck")]
    Success,
    #[strum(serialize = "CircleCheckBig")]
    DoneIssue,
    #[strum(serialize = "CircleDashed")]
    BacklogIssue,
    #[strum(serialize = "CircleDot")]
    ActiveIssue,
    #[strum(serialize = "CircleX")]
    CancelledIssue,
    #[strum(serialize = "Command")]
    KeyboardShortcut,
    Copy,
    Download,
    #[strum(serialize = "Ellipsis")]
    MoreActions,
    #[strum(serialize = "ExternalLink")]
    OpenExternal,
    #[strum(serialize = "Eye")]
    Preview,
    #[strum(serialize = "EyeOff")]
    HidePassword,
    #[strum(serialize = "FileText")]
    Page,
    #[strum(serialize = "Folder")]
    Project,
    #[strum(serialize = "FolderClosed")]
    Folder,
    #[strum(serialize = "FolderMinus")]
    RemoveFolder,
    #[strum(serialize = "FolderPlus")]
    AddFolder,
    #[strum(serialize = "Globe")]
    PublicView,
    History,
    #[strum(serialize = "House")]
    Home,
    Info,
    #[strum(serialize = "Layers")]
    Modules,
    #[strum(serialize = "LayoutDashboard")]
    Overview,
    #[strum(serialize = "LayoutGrid")]
    Board,
    #[strum(serialize = "List")]
    Issues,
    #[strum(serialize = "ListChecks")]
    Plans,
    #[strum(serialize = "Lock")]
    Restricted,
    #[strum(serialize = "LogIn")]
    SignIn,
    #[strum(serialize = "Menu")]
    OpenNavigation,
    #[strum(serialize = "MessageSquare")]
    Comment,
    #[strum(serialize = "Monitor")]
    SystemTheme,
    #[strum(serialize = "Moon")]
    DarkTheme,
    #[strum(serialize = "PanelLeftClose")]
    CollapseSidebar,
    #[strum(serialize = "PanelRight")]
    DetailsPanel,
    #[strum(serialize = "Paperclip")]
    Attachment,
    #[strum(serialize = "Palette")]
    Appearance,
    #[strum(serialize = "Pencil")]
    Edit,
    #[strum(serialize = "Pin")]
    Pinned,
    Plug,
    #[strum(serialize = "Plus")]
    Add,
    Search,
    Settings,
    #[strum(serialize = "Sun")]
    LightTheme,
    #[strum(serialize = "Sunrise")]
    Morning,
    #[strum(serialize = "Sunset")]
    Evening,
    #[strum(serialize = "Tag")]
    Labels,
    #[strum(serialize = "Trash2")]
    Delete,
    #[strum(serialize = "TrendingUp")]
    Insights,
    #[strum(serialize = "UserPlus")]
    AddMember,
    #[strum(serialize = "Users")]
    Members,
    #[strum(serialize = "UsersRound")]
    ProjectMembers,
    #[strum(serialize = "Waypoints")]
    Graph,
    #[strum(serialize = "X")]
    Close,
    #[strum(serialize = "Eye")]
    ShowPassword,
    #[strum(serialize = "Check")]
    Saved,
    #[strum(serialize = "Check")]
    Copied,
    #[strum(serialize = "Check")]
    Complete,
    #[strum(serialize = "Check")]
    AllClear,
    #[strum(serialize = "ChevronRight")]
    BreadcrumbSeparator,
    #[strum(serialize = "Moon")]
    Night,
    #[strum(serialize = "Sun")]
    Day,
    #[strum(serialize = "Circle")]
    TodoIssue,
    #[strum(serialize = "FileText")]
    Pages,
    #[strum(serialize = "Paperclip")]
    Files,
    #[strum(serialize = "History")]
    Activity,
    #[strum(serialize = "CircleDot")]
    IssueLink,
    #[strum(serialize = "Box")]
    Entity,
    #[strum(serialize = "ArrowUpRight")]
    OpenEntity,
}

impl UiIcon {
    pub(super) fn glyph(self) -> &'static str {
        self.into()
    }
}
