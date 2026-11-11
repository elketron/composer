/// What entering a site did about its `app:`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppLaunch {
    /// The site names no application.
    NoApp,
    /// The application was started (`app` is the `[applications.<app>]` name).
    Launched { app: String },
    /// The launching server's `list_apps` already mentions the application's command.
    AlreadyRunning { app: String },
    /// Nothing was started, and why; entering succeeded all the same.
    Skipped { app: String, reason: String },
}

impl AppLaunch {
    /// The outcome in words, for a tool result or a status line. Empty for [`Self::NoApp`].
    pub fn describe(&self) -> String {
        match self {
            Self::NoApp => String::new(),
            Self::Launched { app } => format!("Started the {app} application."),
            Self::AlreadyRunning { app } => format!("The {app} application is already running."),
            Self::Skipped { app, reason } => {
                format!("The {app} application was not started: {reason}")
            }
        }
    }
}
