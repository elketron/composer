use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::McpServerConfig;
use crate::mcp_bundles::McpBundleManifest;
use crate::mcp_bundles::mcp_bundle_file::MARKER;
use crate::mcp_bundles::substitute::substitute;

/// An unpacked bundle: a server name, its manifest and the folder it was unpacked to.
#[derive(Debug, Clone)]
pub struct McpBundle {
    pub name: String,
    pub manifest: McpBundleManifest,
    /// Absolute.
    pub folder: PathBuf,
}

/// The variable a bundle's server finds its writable folder in.
pub const STATE_ENV: &str = "COMPOSER_BUNDLE_STATE";

impl McpBundle {
    /// The server config this bundle describes: `${__dirname}`, `${HOME}` and
    /// `${user_config.KEY}` substituted (a key's value is `settings`, else the manifest's
    /// default). Missing required settings are carried in `problem`, so the server is registered
    /// and fails when it connects.
    ///
    /// The server cannot write its own code: the unpacked folder is bound read-only. What it
    /// writes goes to `run` (`<home>/state/mcps/.run/<name>/`), the one folder bound read-write
    /// and named in [`STATE_ENV`]. A `uv` server gets its environment and its cache there
    /// (`UV_PROJECT_ENVIRONMENT`, `UV_CACHE_DIR`), and starts in a project folder there
    /// ([`Self::uv_project`]), because `uv run` writes `uv.lock` beside `pyproject.toml`;
    /// `${__dirname}` is that folder for it. Every other server starts in the unpacked folder.
    pub fn config(
        &self,
        settings: &BTreeMap<String, String>,
        user_home: &str,
        run: &Path,
    ) -> McpServerConfig {
        let uv = self.is_uv();
        let folder = self.folder.display().to_string();
        let dirname = match uv {
            true => Self::uv_project(run).display().to_string(),
            false => folder.clone(),
        };
        let paths = |key: &str| match key {
            "__dirname" => Some(dirname.clone()),
            "HOME" => Some(user_home.to_string()),
            _ => None,
        };
        let mut values: BTreeMap<String, String> = BTreeMap::new();
        let mut missing = Vec::new();
        for (key, setting) in &self.manifest.user_config {
            let value = settings
                .get(key)
                .cloned()
                .or_else(|| setting.default.as_deref().map(|d| substitute(d, &paths)));
            match value {
                Some(value) => {
                    values.insert(key.clone(), value);
                }
                None if setting.required => missing.push(key.as_str()),
                None => {
                    values.insert(key.clone(), String::new());
                }
            }
        }
        for (key, value) in settings {
            values.entry(key.clone()).or_insert_with(|| value.clone());
        }
        let lookup = |key: &str| {
            paths(key).or_else(|| values.get(key.strip_prefix("user_config.")?).cloned())
        };
        let sub = |text: &str| substitute(text, &lookup);
        let state = run.display().to_string();
        let mut config = McpServerConfig {
            command: sub(&self.manifest.command),
            args: self.manifest.args.iter().map(|a| sub(a)).collect(),
            env: self
                .manifest
                .env
                .iter()
                .map(|(k, v)| (k.clone(), sub(v)))
                .collect(),
            cwd: dirname.clone(),
            // The code read-only (and, before it, nothing else of the bundle read-write): `cwd`
            // is either that folder or inside `run`.
            cwd_read_only: true,
            binds_ro: vec![folder],
            binds_rw: vec![state.clone()],
            enabled: true,
            ..Default::default()
        };
        config.env.insert(STATE_ENV.into(), state);
        if uv {
            for (variable, folder) in [
                ("UV_PROJECT_ENVIRONMENT", "venv"),
                ("UV_CACHE_DIR", "uv-cache"),
            ] {
                config
                    .env
                    .insert(variable.into(), run.join(folder).display().to_string());
            }
        }
        if !missing.is_empty() {
            config.problem = Some(format!(
                "the bundle needs settings: {}; set them under [mcp.{}.config] in config.toml",
                missing.join(", "),
                self.name
            ));
        }
        config
    }

    fn is_uv(&self) -> bool {
        self.manifest.server_type == "uv"
    }

    /// Where a `uv` bundle's server starts, inside its `run` folder.
    pub fn uv_project(run: &Path) -> PathBuf {
        run.join("project")
    }

    /// Makes `run` ready before the server starts: the folder itself, and for a `uv` bundle its
    /// project folder: every top-level file of the unpacked bundle copied, every top-level folder
    /// a link to the read-only one. It is made again each time, so nothing a server left there
    /// stays except `uv.lock` (which `uv` wrote; a bundle's own replaces it) and the build's
    /// `*.egg-info`.
    pub fn prepare(&self, run: &Path) -> Result<(), String> {
        let io = |e: std::io::Error| format!("preparing {}: {e}", run.display());
        std::fs::create_dir_all(run).map_err(io)?;
        if !self.is_uv() {
            return Ok(());
        }
        let project = Self::uv_project(run);
        std::fs::create_dir_all(&project).map_err(io)?;
        let mut ours = Vec::new();
        for entry in std::fs::read_dir(&self.folder).map_err(io)? {
            let entry = entry.map_err(io)?;
            let name = entry.file_name();
            // A build's own output, left in the unpacked folder by a server that ran from it
            // before the code became read-only: linking it would make the build write through
            // the link into the read-only folder and fail.
            if name == MARKER || name.to_string_lossy().ends_with(".egg-info") {
                continue;
            }
            let (from, to) = (entry.path(), project.join(&name));
            let kind = entry.file_type().map_err(io)?;
            if kind.is_dir() {
                if std::fs::read_link(&to).ok().as_deref() != Some(from.as_path()) {
                    remove(&to).map_err(io)?;
                    std::os::unix::fs::symlink(&from, &to).map_err(io)?;
                }
            } else if kind.is_file() {
                let wanted = std::fs::read(&from).map_err(io)?;
                let plain = to.symlink_metadata().is_ok_and(|m| m.is_file());
                // Left alone when it is right: a rewrite would make `uv` build again.
                if !plain || std::fs::read(&to).ok().as_deref() != Some(wanted.as_slice()) {
                    remove(&to).map_err(io)?;
                    std::fs::write(&to, wanted).map_err(io)?;
                }
            } else {
                continue;
            }
            ours.push(name);
        }
        for entry in std::fs::read_dir(&project).map_err(io)? {
            let entry = entry.map_err(io)?;
            let name = entry.file_name();
            // `file_type` does not follow links: a linked `uv.lock` or `*.egg-info` goes.
            let kind = entry.file_type().ok();
            let kept = name == "uv.lock" && kind.is_some_and(|kind| kind.is_file())
                || name.to_string_lossy().ends_with(".egg-info")
                    && kind.is_some_and(|kind| kind.is_dir());
            if !kept && !ours.contains(&name) {
                remove(&entry.path()).map_err(io)?;
            }
        }
        Ok(())
    }

    /// The config of `[mcp.<name>]` given the bundle: no entry, the bundle alone; an entry
    /// without `command` or `url`, the bundle with the entry on top; an entry with its own,
    /// that entry unchanged (`None`: the bundle is ignored).
    pub fn merged(
        &self,
        entry: Option<&McpServerConfig>,
        user_home: &str,
        run: &Path,
    ) -> Option<McpServerConfig> {
        let Some(entry) = entry else {
            return Some(self.config(&BTreeMap::new(), user_home, run));
        };
        if !entry.command.is_empty() || !entry.url.is_empty() {
            return None;
        }
        let mut config = self.config(&entry.config, user_home, run);
        config.env.extend(entry.env.clone());
        config.binds_ro.extend(entry.binds_ro.clone());
        config.binds_rw.extend(entry.binds_rw.clone());
        config.allow = entry.allow.clone();
        config.sites = entry.sites.clone();
        config.enabled = entry.enabled;
        config.config = entry.config.clone();
        Some(config)
    }
}

/// Removes whatever is at `path` (a file, a link, a folder); nothing there is fine.
fn remove(path: &Path) -> std::io::Result<()> {
    match path.symlink_metadata() {
        Ok(meta) if meta.is_dir() => std::fs::remove_dir_all(path),
        Ok(_) => std::fs::remove_file(path),
        Err(_) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bundle(manifest: &str) -> McpBundle {
        McpBundle {
            name: "demo".into(),
            manifest: McpBundleManifest::parse(manifest).unwrap(),
            folder: PathBuf::from("/state/mcps/demo"),
        }
    }

    const MANIFEST: &str = r#"{ "name": "Demo", "server": { "type": "python", "mcp_config": {
        "command": "python3", "args": ["${__dirname}/server.py", "--key=${user_config.key}", "${user_config.extra}"],
        "env": { "HOME_DIR": "${HOME}/x", "PLAIN": "1" },
        "platform_overrides": { "linux": { "env": { "HOME_DIR": "${HOME}/linux" } } }
    } }, "user_config": { "key": { "default": "dflt" }, "extra": {} } }"#;

    fn config(b: &McpBundle, settings: &[(&str, &str)]) -> McpServerConfig {
        let settings = settings
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        b.config(&settings, "/home/me", Path::new("/state/mcps/.run/demo"))
    }

    #[test]
    fn manifest_becomes_config() {
        let b = bundle(MANIFEST);
        let c = config(&b, &[]);
        assert_eq!(c.command, "python3");
        assert_eq!(c.args, ["/state/mcps/demo/server.py", "--key=dflt", ""]);
        assert!(
            !c.env.contains_key("PLAIN"),
            "the linux env replaces the base one"
        );
        assert_eq!(c.env["HOME_DIR"], "/home/me/linux");
        assert_eq!(c.cwd, "/state/mcps/demo");
        assert!(c.enabled && c.allow.is_empty() && c.problem.is_none());
        let c = config(&b, &[("key", "mine")]);
        assert_eq!(c.args[1], "--key=mine");
    }

    #[test]
    fn the_code_is_read_only_and_writes_go_to_the_run_folder() {
        let c = config(&bundle(MANIFEST), &[]);
        assert_eq!(c.cwd, "/state/mcps/demo");
        assert!(c.cwd_read_only);
        assert_eq!(c.binds_ro, ["/state/mcps/demo"]);
        assert_eq!(c.binds_rw, ["/state/mcps/.run/demo"]);
        assert_eq!(c.env[STATE_ENV], "/state/mcps/.run/demo");
        assert!(!c.env.contains_key("UV_CACHE_DIR"));

        // `uv`: its environment and cache there too, and it starts in a project folder there.
        let b = bundle(
            r#"{ "name": "u", "server": { "type": "uv", "mcp_config": { "command": "uv",
                "args": ["run", "--directory", "${__dirname}", "x"],
                "env": { "UV_CACHE_DIR": "/elsewhere" } } } }"#,
        );
        let c = config(&b, &[]);
        assert_eq!(
            c.env["UV_PROJECT_ENVIRONMENT"],
            "/state/mcps/.run/demo/venv"
        );
        assert_eq!(c.env["UV_CACHE_DIR"], "/state/mcps/.run/demo/uv-cache");
        assert_eq!(c.env[STATE_ENV], "/state/mcps/.run/demo");
        assert_eq!(c.cwd, "/state/mcps/.run/demo/project");
        assert_eq!(c.args[2], "/state/mcps/.run/demo/project");
        assert!(c.cwd_read_only);
        assert_eq!(c.binds_ro, ["/state/mcps/demo"]);
        assert_eq!(c.binds_rw, ["/state/mcps/.run/demo"]);
    }

    #[test]
    fn a_uv_project_is_made_again_from_the_unpacked_bundle() {
        let dir = tempfile::tempdir().unwrap();
        let (folder, run) = (dir.path().join("code"), dir.path().join(".run/u"));
        std::fs::create_dir_all(folder.join("pkg")).unwrap();
        std::fs::write(folder.join("pkg/__init__.py"), "x = 1").unwrap();
        std::fs::write(folder.join("pyproject.toml"), "[project]").unwrap();
        std::fs::write(folder.join(MARKER), "hash").unwrap();
        let b = McpBundle {
            name: "u".into(),
            manifest: McpBundleManifest::parse(
                r#"{ "name": "u", "server": { "type": "uv", "mcp_config": { "command": "uv" } } }"#,
            )
            .unwrap(),
            folder: folder.clone(),
        };
        b.prepare(&run).unwrap();
        let project = McpBundle::uv_project(&run);
        let read = |name: &str| std::fs::read_to_string(project.join(name)).unwrap();
        assert_eq!(read("pyproject.toml"), "[project]");
        assert_eq!(read("pkg/__init__.py"), "x = 1");
        assert_eq!(
            std::fs::read_link(project.join("pkg")).unwrap(),
            folder.join("pkg")
        );
        assert!(!project.join(MARKER).exists());

        // What a server left or changed there is gone at the next start, except uv's lock and
        // the build's egg-info; the unpacked code was never touched.
        std::fs::write(project.join("pyproject.toml"), "[evil]").unwrap();
        std::fs::write(project.join("sitecustomize.py"), "evil").unwrap();
        std::fs::create_dir_all(project.join("yaml")).unwrap();
        std::fs::write(project.join("uv.lock"), "lock").unwrap();
        std::fs::create_dir_all(project.join("u.egg-info")).unwrap();
        std::fs::remove_file(project.join("pkg")).unwrap();
        std::fs::create_dir_all(project.join("pkg")).unwrap();
        std::fs::write(project.join("pkg/__init__.py"), "evil").unwrap();
        b.prepare(&run).unwrap();
        assert_eq!(read("pyproject.toml"), "[project]");
        assert_eq!(read("pkg/__init__.py"), "x = 1");
        assert_eq!(read("uv.lock"), "lock");
        assert!(project.join("u.egg-info").is_dir());
        assert!(!project.join("sitecustomize.py").exists() && !project.join("yaml").exists());
        assert_eq!(
            std::fs::read_to_string(folder.join("pyproject.toml")).unwrap(),
            "[project]"
        );
        // A link planted as the lock is not kept.
        std::fs::remove_file(project.join("uv.lock")).unwrap();
        std::os::unix::fs::symlink(folder.join("pyproject.toml"), project.join("uv.lock")).unwrap();
        b.prepare(&run).unwrap();
        assert!(project.join("uv.lock").symlink_metadata().is_err());

        // An egg-info a server left in the unpacked folder before it was read-only is not
        // linked (the build would write through the link); an old link to it goes.
        std::fs::create_dir_all(folder.join("u.egg-info")).unwrap();
        std::fs::remove_dir_all(project.join("u.egg-info")).unwrap();
        std::os::unix::fs::symlink(folder.join("u.egg-info"), project.join("u.egg-info")).unwrap();
        b.prepare(&run).unwrap();
        assert!(project.join("u.egg-info").symlink_metadata().is_err());

        // Another kind of bundle only gets the folder.
        let mut plain = b.clone();
        plain.manifest.server_type = "python".into();
        let other = dir.path().join(".run/p");
        plain.prepare(&other).unwrap();
        assert!(other.is_dir() && !McpBundle::uv_project(&other).exists());
    }

    #[test]
    fn missing_required_settings_are_a_problem() {
        let b = bundle(
            r#"{ "name": "d", "server": { "mcp_config": { "command": "x" } },
                "user_config": { "a": { "required": true }, "b": { "required": true, "default": "1" },
                                 "c": { "required": true } } }"#,
        );
        let problem = config(&b, &[]).problem.unwrap();
        assert!(
            problem.contains("a, c") && !problem.contains("b,"),
            "{problem}"
        );
        assert!(problem.contains("[mcp.demo.config]"));
        assert!(config(&b, &[("a", "1"), ("c", "2")]).problem.is_none());
    }

    #[test]
    fn merging_with_config_toml() {
        let b = bundle(MANIFEST);
        let cache = Path::new("/state/mcps/.run/demo");
        // No entry: the bundle registers itself.
        let c = b.merged(None, "/h", cache).unwrap();
        assert!(c.enabled && c.allow.is_empty() && !c.command.is_empty());
        assert!(c.sites.is_empty(), "alone, no site may grant it");
        // An entry without command or url overlays it.
        let entry = McpServerConfig {
            env: [("PLAIN".to_string(), "2".to_string())].into(),
            binds_ro: vec!["/ro".into()],
            allow: vec!["get_*".into()],
            sites: vec!["3d".into()],
            enabled: false,
            config: [("key".to_string(), "k".to_string())].into(),
            ..Default::default()
        };
        let c = b.merged(Some(&entry), "/h", cache).unwrap();
        assert_eq!(c.command, "python3");
        assert_eq!(c.cwd, "/state/mcps/demo");
        assert_eq!(c.args[1], "--key=k");
        assert_eq!(c.env["PLAIN"], "2");
        assert_eq!(c.env["HOME_DIR"], "/h/linux");
        assert_eq!(c.binds_ro, ["/state/mcps/demo", "/ro"]);
        assert_eq!((c.allow, c.enabled), (vec!["get_*".to_string()], false));
        assert_eq!(c.sites, ["3d"], "the overlay says which sites may grant it");
        assert_eq!(c.config["key"], "k");
        // An entry with its own command wins.
        let own = McpServerConfig {
            command: "mine".into(),
            ..Default::default()
        };
        assert!(b.merged(Some(&own), "/h", cache).is_none());
        let own = McpServerConfig {
            url: "http://x".into(),
            ..Default::default()
        };
        assert!(b.merged(Some(&own), "/h", cache).is_none());
    }
}
