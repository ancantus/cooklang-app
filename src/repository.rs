use std::{
    fs, io,
    num::NonZeroU32,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

use cooklang::{CooklangParser, Converter, Extensions, Recipe, metadata::{CooklangValueExt, StdKey}};

/// A Cooklang file found in a repository checkout.
#[derive(Debug)]
pub struct RecipeFile {
    /// Path relative to the checkout root.
    pub path: PathBuf,
    /// Metadata title, or the file name when no title is present.
    pub title: String,
    /// Parsed content, when the file uses syntax supported by the parser.
    pub recipe: Result<Recipe, String>,
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
    use gix::credentials::{
        helper::Action,
        protocol::Outcome as CredentialOutcome,
    };

    let token = user_access_token.map(str::to_owned);
    let mut clone = gix::prepare_clone(url, destination)?
        .with_shallow(gix::remote::fetch::Shallow::DepthAtRemote(
            NonZeroU32::new(1).expect("one is non-zero"),
        ))
        .configure_connection(move |connection| {
            let token = token.clone();
            connection.set_credentials(move |action| match action {
                Action::Get(context)
                    if context.host.as_deref() == Some("github.com")
                        && token.is_some() =>
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
    let mut recipes = paths.into_iter().map(|path| {
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

        Ok(RecipeFile { path, title, recipe })
    }).collect::<io::Result<Vec<_>>>()?;
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
        } else if file_type.is_file() && path.extension().is_some_and(|extension| extension == "cook") {
            if let Ok(relative_path) = path.strip_prefix(root) {
                paths.push(relative_path.to_owned());
            }
        }
    }
    Ok(())
}
