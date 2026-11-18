use std::{
    fs, io,
    num::NonZeroU32,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::{SystemTime, UNIX_EPOCH},
};

use cooklang::{
    Converter, CooklangParser, Extensions, Recipe,
    metadata::{CooklangValueExt, StdKey},
};

/// A Cooklang file found in a repository checkout.
#[derive(Debug, Clone, PartialEq)]
pub struct RecipeFile {
    /// Path relative to the checkout root.
    pub path: PathBuf,
    /// Metadata title, or the file name when no title is present.
    pub title: String,
    /// Original source, retained for readable fallback when parsing fails.
    pub source: String,
    /// Parsed content, when the file uses syntax supported by the parser.
    pub recipe: Result<Recipe, String>,
}

/// A connected repository and the recipes currently available from its checkout.
#[derive(Debug, Clone, PartialEq)]
pub struct Collection {
    pub url: String,
    pub path: PathBuf,
    pub recipes: Vec<RecipeFile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GithubRepository {
    url: String,
    owner: String,
    name: String,
}

/// Read the last connected checkout, if one was saved on this device.
pub fn load_saved_collection() -> Result<Option<Collection>, String> {
    let saved_url = settings_path();
    if !saved_url.is_file() {
        return Ok(None);
    }
    let url = fs::read_to_string(saved_url).map_err(|error| error.to_string())?;
    let repository = normalize_github_url(&url)?;
    let path = collection_path(&repository.owner, &repository.name)?;
    if !path.is_dir() {
        return Ok(None);
    }
    let recipes = read_recipes(&path).map_err(|error| error.to_string())?;
    Ok(Some(Collection {
        url: repository.url,
        path,
        recipes,
    }))
}

/// Connect to a GitHub repository or refresh its existing checkout.
pub fn connect_or_refresh(input: &str) -> Result<Collection, String> {
    let repository = normalize_github_url(input)?;
    let destination = collection_path(&repository.owner, &repository.name)?;
    if destination.exists() {
        refresh_checkout(&repository.url, &destination)?;
    } else {
        fs::create_dir_all(destination.parent().expect("collection path has a parent"))
            .map_err(|error| error.to_string())?;
        clone_github_repository(&repository.url, &destination, None)
            .map_err(|error| error.to_string())?;
    }

    let recipes = read_recipes(&destination).map_err(|error| error.to_string())?;
    save_url(&repository.url)?;
    Ok(Collection {
        url: repository.url,
        path: destination,
        recipes,
    })
}

/// Remove a connected checkout and its saved repository selection.
pub fn disconnect(url: &str) -> Result<(), String> {
    let repository = normalize_github_url(url)?;
    let path = collection_path(&repository.owner, &repository.name)?;
    if path.exists() {
        fs::remove_dir_all(path).map_err(|error| error.to_string())?;
    }
    let saved_url = settings_path();
    if saved_url.exists() {
        fs::remove_file(saved_url).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn normalize_github_url(input: &str) -> Result<GithubRepository, String> {
    const HOST_PREFIXES: [&str; 6] = [
        "https://github.com/",
        "https://www.github.com/",
        "http://github.com/",
        "http://www.github.com/",
        "github.com/",
        "www.github.com/",
    ];
    let input = input.trim();
    let strip_host_prefix = || {
        HOST_PREFIXES
            .iter()
            .find_map(|prefix| input.strip_prefix(*prefix))
            .unwrap_or(input)
    };
    let path = {
        let path = strip_host_prefix().trim_end_matches('/');
        path.strip_suffix(".git").unwrap_or(path)
    };
    let mut parts = path.split('/');
    let owner = parts.next().unwrap_or_default();
    let name = parts.next().unwrap_or_default();
    let valid_segment = |part: &str| {
        !part.is_empty()
            && part != "."
            && part != ".."
            && part
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || "-_.".contains(character))
    };
    if parts.next().is_some() || !valid_segment(owner) || !valid_segment(name) {
        return Err(
            "Enter a GitHub repository as owner/repository or a github.com URL.".to_owned(),
        );
    }
    Ok(GithubRepository {
        url: format!("https://github.com/{owner}/{name}.git"),
        owner: owner.to_owned(),
        name: name.to_owned(),
    })
}

#[cfg(test)]
#[path = "repository_tests.rs"]
mod tests;

fn app_data_dir() -> Result<PathBuf, String> {
    let project = robius_directories::ProjectDirs::from("org", "cooklang", "Cooklang Client")
        .ok_or_else(|| "The app data directory is unavailable on this device.".to_owned())?;
    Ok(project.data_local_dir().to_owned())
}

fn collection_path(owner: &str, name: &str) -> Result<PathBuf, String> {
    Ok(app_data_dir()?.join("collections").join(owner).join(name))
}

fn settings_path() -> PathBuf {
    app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir().join("cooklang-client"))
        .join("connected-repository")
}

fn save_url(url: &str) -> Result<(), String> {
    let path = settings_path();
    fs::create_dir_all(path.parent().expect("settings file has a parent"))
        .map_err(|error| error.to_string())?;
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, url).map_err(|error| error.to_string())?;
    fs::rename(temporary, path).map_err(|error| error.to_string())
}

fn refresh_checkout(url: &str, destination: &Path) -> Result<(), String> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let temporary = destination.with_file_name(format!("refresh-{nonce}"));
    let backup = destination.with_file_name(format!("previous-{nonce}"));
    clone_github_repository(url, &temporary, None).map_err(|error| error.to_string())?;
    fs::rename(destination, &backup).map_err(|error| error.to_string())?;
    if let Err(error) = fs::rename(&temporary, destination) {
        let _ = fs::rename(&backup, destination);
        return Err(error.to_string());
    }
    fs::remove_dir_all(backup).map_err(|error| error.to_string())
}

/// Shallow-clone a GitHub repository into `destination`.
///
/// The optional user access token is held only by the credential callback and
/// is never written to the remote URL or repository configuration.
pub fn clone_github_repository(
    url: &str,
    destination: impl AsRef<Path>,
    user_access_token: Option<&str>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use gix::credentials::{helper::Action, protocol::Outcome as CredentialOutcome};

    let token = user_access_token.map(str::to_owned);
    let mut clone = gix::prepare_clone(url, destination)?
        .with_shallow(gix::remote::fetch::Shallow::DepthAtRemote(
            NonZeroU32::new(1).expect("one is non-zero"),
        ))
        .configure_connection(move |connection| {
            let token = token.clone();
            connection.set_credentials(move |action| match action {
                Action::Get(context)
                    if context.host.as_deref() == Some("github.com") && token.is_some() =>
                {
                    let token = token.as_ref().expect("checked above");
                    let next = context.into();
                    Ok(Some(CredentialOutcome {
                        identity: gix::sec::identity::Account {
                            username: "x-access-token".to_owned(),
                            password: token.clone(),
                            oauth_refresh_token: None,
                        },
                        next,
                    }))
                }
                Action::Get(_) | Action::Store(_) | Action::Erase(_) => Ok(None),
            });
            Ok(())
        });

    let interrupted = AtomicBool::new(false);
    let (mut checkout, _) = clone.fetch_then_checkout(gix::progress::Discard, &interrupted)?;
    checkout.main_worktree(gix::progress::Discard, &interrupted)?;
    Ok(())
}

/// Recursively find and parse lowercase `.cook` files under `root`.
///
/// Paths are sorted to provide stable ordering. Files that fail to parse remain
/// in the result with their error so callers can display their source later.
pub fn read_recipes(root: impl AsRef<Path>) -> io::Result<Vec<RecipeFile>> {
    let root = root.as_ref();
    let mut paths = Vec::new();
    collect_recipe_paths(root, root, &mut paths)?;
    paths.sort();

    let parser = CooklangParser::new(Extensions::empty(), Converter::default());
    let mut recipes = paths
        .into_iter()
        .map(|path| {
            let source = fs::read_to_string(root.join(&path))?;
            let parsed = parser.parse(&source);
            let title = parsed
                .output()
                .and_then(|recipe| recipe.metadata.get(StdKey::Title))
                .and_then(CooklangValueExt::as_str_like)
                .map(|title| title.into_owned())
                .unwrap_or_else(|| {
                    path.file_stem()
                        .and_then(|name| name.to_str())
                        .unwrap_or("Untitled recipe")
                        .to_owned()
                });
            let recipe = parsed
                .into_result()
                .map(|(recipe, _warnings)| recipe)
                .map_err(|error| error.to_string());

            Ok(RecipeFile {
                path,
                title,
                source,
                recipe,
            })
        })
        .collect::<io::Result<Vec<_>>>()?;
    recipes.sort_by(|left, right| {
        left.title
            .to_lowercase()
            .cmp(&right.title.to_lowercase())
            .then_with(|| left.path.cmp(&right.path))
    });
    Ok(recipes)
}

fn collect_recipe_paths(root: &Path, directory: &Path, paths: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let path = entry.path();
        if file_type.is_dir() {
            collect_recipe_paths(root, &path, paths)?;
        } else if file_type.is_file()
            && path
                .extension()
                .is_some_and(|extension| extension == "cook")
        {
            if let Ok(relative_path) = path.strip_prefix(root) {
                paths.push(relative_path.to_owned());
            }
        }
    }
    Ok(())
}
