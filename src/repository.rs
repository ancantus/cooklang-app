use std::{fs, io, path::{Path, PathBuf}};

use cooklang::{CooklangParser, Recipe, metadata::{CooklangValueExt, StdKey}};

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

/// Recursively find and parse lowercase `.cook` files under `root`.
///
/// Paths are sorted to provide stable ordering. Files that fail to parse remain
/// in the result with their error so callers can display their source later.
pub fn read_recipes(root: impl AsRef<Path>) -> io::Result<Vec<RecipeFile>> {
    let root = root.as_ref();
    let mut paths = Vec::new();
    collect_recipe_paths(root, root, &mut paths)?;
    paths.sort();

    let parser = CooklangParser::default();
    paths.into_iter().map(|path| {
        let source = fs::read_to_string(root.join(&path))?;
        let parsed = parser.parse(&source);
        let title = parsed
            .as_ref()
            .ok()
            .and_then(|(recipe, _)| recipe.metadata.get(StdKey::Title))
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
    }).collect()
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
