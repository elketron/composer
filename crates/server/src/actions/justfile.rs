use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use tokio::process::Command;

use crate::actions::Recipe;
use crate::hash::content_hash;

/// A site's justfile, read through `just --dump --dump-format json`.
#[derive(Debug, Clone)]
pub struct Justfile {
    dir: PathBuf,
    dump: Value,
}

impl Justfile {
    /// Reads `<site_dir>/justfile`. Errors if it is missing or `just` can't parse it.
    pub async fn load(site_dir: &Path) -> Result<Self, String> {
        if !site_dir.join("justfile").is_file() {
            return Err("this site has no justfile, so no actions".into());
        }
        let output = just(site_dir)
            .args(["--dump", "--dump-format", "json"])
            .output()
            .await
            .map_err(just_missing)?;
        if !output.status.success() {
            return Err(format!(
                "justfile error: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        let dump = serde_json::from_slice(&output.stdout)
            .map_err(|e| format!("reading just --dump: {e}"))?;
        Ok(Self {
            dir: site_dir.to_path_buf(),
            dump,
        })
    }

    pub fn recipes(&self) -> Vec<Recipe> {
        let Some(recipes) = self.dump["recipes"].as_object() else {
            return Vec::new();
        };
        recipes.values().map(recipe_info).collect()
    }

    pub fn recipe(&self, name: &str) -> Option<Recipe> {
        self.dump["recipes"].get(name).map(recipe_info)
    }

    /// Identifies this version of a recipe: the recipe, everything it depends on,
    /// and the justfile's variables and settings. Any change means a new approval.
    pub fn hash(&self, name: &str) -> String {
        let mut recipes = serde_json::Map::new();
        let mut todo = vec![name.to_string()];
        while let Some(next) = todo.pop() {
            if recipes.contains_key(&next) {
                continue;
            }
            let recipe = self.dump["recipes"]
                .get(&next)
                .cloned()
                .unwrap_or(Value::Null);
            for dep in recipe["dependencies"].as_array().into_iter().flatten() {
                if let Some(dep) = dep["recipe"].as_str() {
                    todo.push(dep.to_string());
                }
            }
            recipes.insert(next, recipe);
        }
        let identity = json!({
            "recipes": recipes,
            "assignments": self.dump["assignments"],
            "settings": self.dump["settings"],
        });
        content_hash(identity.to_string().as_bytes())
    }

    /// Source of the recipe and its dependencies, as shown to the user for approval.
    pub async fn show(&self, name: &str) -> Result<String, String> {
        let mut names = vec![name.to_string()];
        let mut i = 0;
        while i < names.len() {
            let deps = self.dump["recipes"][&names[i]]["dependencies"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            for dep in deps.iter().filter_map(|d| d["recipe"].as_str()) {
                if !names.iter().any(|n| n == dep) {
                    names.push(dep.to_string());
                }
            }
            i += 1;
        }
        let mut parts = Vec::new();
        for recipe in names {
            let output = just(&self.dir)
                .args(["--show", &recipe])
                .output()
                .await
                .map_err(just_missing)?;
            parts.push(
                String::from_utf8_lossy(&output.stdout)
                    .trim_end()
                    .to_string(),
            );
        }
        let assignments = self.dump["assignments"].as_object().map_or(0, |a| a.len());
        if assignments > 0 {
            parts.push(format!(
                "# + {assignments} justfile variable(s), included in the approval"
            ));
        }
        Ok(parts.join("\n\n"))
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

/// `just` pointed at a site's justfile, run from the site folder.
pub fn just(site_dir: &Path) -> Command {
    let mut command = Command::new("just");
    command
        .arg("--justfile")
        .arg(site_dir.join("justfile"))
        .arg("--working-directory")
        .arg(site_dir);
    command
}

fn just_missing(e: std::io::Error) -> String {
    format!("could not run `just` ({e}); install it to use site actions")
}

fn recipe_info(recipe: &Value) -> Recipe {
    let name = recipe["name"].as_str().unwrap_or_default().to_string();
    let private = recipe["private"].as_bool().unwrap_or(false)
        || name.starts_with('_')
        || recipe["attributes"]
            .as_array()
            .is_some_and(|a| a.iter().any(|x| x == "private"));
    let parameters = recipe["parameters"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|p| {
            let name = p["name"].as_str().unwrap_or_default();
            let variadic = if p["kind"].as_str().is_some_and(|k| k != "singular") {
                "*"
            } else {
                ""
            };
            match &p["default"] {
                Value::Null => format!("{variadic}{name}"),
                default => format!("{variadic}{name}={}", default_text(default)),
            }
        })
        .collect();
    Recipe {
        doc: recipe["doc"].as_str().map(str::to_string),
        parameters,
        private,
        name,
    }
}

fn default_text(value: &Value) -> String {
    match value {
        Value::String(s) => format!("'{s}'"),
        other => other.to_string(),
    }
}
