use std::path::PathBuf;

use dioxus::prelude::*;

use crate::repository::{self, Collection, RecipeFile};

#[derive(Default)]
struct BrowserState {
    collection: Option<Collection>,
    repository_input: String,
    search: String,
    selected_recipe: Option<PathBuf>,
    loading: bool,
    stale: bool,
    error: Option<String>,
}

impl BrowserState {
    fn load() -> Self {
        match repository::load_saved_collection() {
            Ok(Some(collection)) => Self {
                repository_input: collection.url.clone(),
                collection: Some(collection),
                stale: true,
                ..Self::default()
            },
            Ok(None) => Self::default(),
            Err(error) => Self {
                error: Some(error),
                ..Self::default()
            },
        }
    }
}

#[component]
pub fn App() -> Element {
    let mut state = use_signal(BrowserState::load);
    let snapshot = state.read();

    rsx! {
        style { {include_str!("../assets/main.css")} }
        main { class: "app-shell",
            header { class: "app-header",
                div { class: "brand-mark", "C" }
                div {
                    p { class: "eyebrow", "COOKLANG COLLECTION" }
                    h1 { "Cooklang" }
                }
            }

            if let Some(error) = snapshot.error.as_ref() {
                section { class: "notice notice-error", role: "alert",
                    strong { "Something went wrong" }
                    p { "{error}" }
                }
            }

            if let Some(collection) = snapshot.collection.as_ref() {
                if let Some(selected_path) = snapshot.selected_recipe.as_ref() {
                    RecipeDetail {
                        recipe: collection.recipes.iter().find(|recipe| &recipe.path == selected_path).cloned(),
                        on_back: move |_| state.write().selected_recipe = None,
                    }
                } else {
                    CollectionBrowser {
                        collection: collection.clone(),
                        search: snapshot.search.clone(),
                        loading: snapshot.loading,
                        stale: snapshot.stale,
                        on_search: move |value| state.write().search = value,
                        on_refresh: move |_| refresh_collection(state),
                        on_disconnect: move |_| disconnect_collection(state),
                        on_select: move |path| state.write().selected_recipe = Some(path),
                    }
                }
            } else {
                RepositorySetup {
                    input: snapshot.repository_input.clone(),
                    loading: snapshot.loading,
                    on_input: move |value| state.write().repository_input = value,
                    on_connect: move |_| connect_collection(state),
                }
            }
        }
    }
}

#[component]
fn RepositorySetup(
    input: String,
    loading: bool,
    on_input: EventHandler<String>,
    on_connect: EventHandler<()>,
) -> Element {
    rsx! {
        section { class: "welcome-card",
            span { class: "section-label", "YOUR COOKBOOK" }
            h2 { "Bring your recipes together." }
            p { class: "muted-copy", "Connect a public GitHub repository to browse your Cooklang recipes." }
            form { class: "connect-form", onsubmit: move |event| { event.prevent_default(); on_connect.call(()); },
                label { r#for: "repository-url", "GitHub repository" }
                input {
                    id: "repository-url",
                    r#type: "url",
                    placeholder: "https://github.com/owner/recipes",
                    value: input.clone(),
                    oninput: move |event| on_input.call(event.value()),
                    disabled: loading,
                }
                button { class: "primary-button", r#type: "submit", disabled: loading || input.trim().is_empty(),
                    if loading { "Connecting…" } else { "Connect repository" }
                }
                p { class: "helper-copy", "Your checkout stays on this device. Public repositories are supported in this prototype." }
            }
        }
    }
}

#[component]
fn CollectionBrowser(
    collection: Collection,
    search: String,
    loading: bool,
    stale: bool,
    on_search: EventHandler<String>,
    on_refresh: EventHandler<MouseEvent>,
    on_disconnect: EventHandler<MouseEvent>,
    on_select: EventHandler<PathBuf>,
) -> Element {
    let query = search.trim().to_lowercase();
    let recipes = collection.recipes.iter().filter(|recipe| {
        query.is_empty()
            || recipe.title.to_lowercase().contains(&query)
                            || recipe_tags(recipe).as_ref().is_some_and(|tags| tags.iter().any(|tag| tag.to_lowercase().contains(&query)))
    }).map(|recipe| {
        let duplicate_title = collection.recipes.iter().filter(|candidate| candidate.title.eq_ignore_ascii_case(&recipe.title)).count() > 1;
        (recipe.clone(), duplicate_title, recipe.path.clone())
    }).collect::<Vec<_>>();

    rsx! {
        section { class: "collection-heading",
            div {
                span { class: "section-label", "YOUR COOKBOOK" }
                h2 { "Recipes" }
                p { class: "repository-path", "{collection.url}" }
            }
            div { class: "collection-actions",
                button { class: "secondary-button", onclick: move |event| on_refresh.call(event), disabled: loading,
                    if loading { "Refreshing…" } else { "Refresh" }
                }
                button { class: "quiet-button", onclick: move |event| on_disconnect.call(event), disabled: loading, "Disconnect" }
            }
        }

        if stale {
            section { class: "notice notice-stale",
                span { class: "status-dot" }
                p { "Showing the last saved collection. Refresh to check for updates." }
            }
        }

        section { class: "recipe-browser",
            div { class: "search-box",
                span { class: "search-icon", "⌕" }
                input {
                    r#type: "search",
                    placeholder: "Search recipes or tags",
                    value: search,
                    oninput: move |event| on_search.call(event.value()),
                }
            }

            if collection.recipes.is_empty() {
                EmptyState {
                    title: "No recipes found yet",
                    detail: "This repository has no .cook files. Add recipes anywhere in the repository, then refresh.",
                }
            } else if recipes.is_empty() {
                EmptyState {
                    title: "No matching recipes",
                    detail: "Try another title or tag.",
                }
            } else {
                div { class: "recipe-list",
                    for (recipe, duplicate_title, path) in recipes {
                        RecipeRow {
                            recipe,
                            duplicate_title,
                            onclick: move |_| on_select.call(path.clone()),
                        }
                    }
                }
            }
            if !loading && collection.recipes.is_empty() {
                p { class: "empty-footnote", "The repository is connected and ready for your recipes." }
            }
        }
    }
}

#[component]
fn RecipeRow(recipe: RecipeFile, duplicate_title: bool, onclick: EventHandler<MouseEvent>) -> Element {
    let tags = recipe_tags(&recipe);
    let time = recipe_time(&recipe);
    rsx! {
        button { class: "recipe-row", onclick: move |event| onclick.call(event),
            div { class: "recipe-row-main",
                h3 { "{recipe.title}" }
                if duplicate_title {
                    p { class: "recipe-file-path", "{recipe.path.display()}" }
                }
                if let Some(tags) = tags {
                    div { class: "tag-list",
                        for tag in tags { span { class: "tag", "{tag}" } }
                    }
                }
            }
            if let Some(time) = time {
                span { class: "recipe-time", "◷ {time} min" }
            }
            span { class: "row-chevron", "›" }
        }
    }
}

#[component]
fn EmptyState(title: &'static str, detail: &'static str) -> Element {
    rsx! {
        div { class: "empty-state",
            span { class: "empty-icon", "✳" }
            h3 { "{title}" }
            p { "{detail}" }
        }
    }
}

#[component]
fn RecipeDetail(recipe: Option<RecipeFile>, on_back: EventHandler<MouseEvent>) -> Element {
    rsx! {
        button { class: "quiet-button back-button", onclick: move |event| on_back.call(event), "‹ Back to recipes" }
        if let Some(recipe) = recipe {
            section { class: "detail-card",
                p { class: "section-label", "RECIPE" }
                h2 { "{recipe.title}" }
                p { class: "recipe-file-path", "{recipe.path.display()}" }
                if let Err(error) = recipe.recipe {
                    div { class: "notice notice-error", "This recipe could not be parsed: {error}" }
                } else {
                    p { class: "muted-copy", "Recipe details are being prepared." }
                }
            }
        } else {
            EmptyState { title: "Recipe unavailable", detail: "Go back to the collection and open another recipe." }
        }
    }
}

fn connect_collection(state: Signal<BrowserState>) {
    let url = state.read().repository_input.clone();
    run_collection_operation(state, move || repository::connect_or_refresh(&url));
}

fn refresh_collection(state: Signal<BrowserState>) {
    let Some(url) = state.read().collection.as_ref().map(|collection| collection.url.clone()) else {
        return;
    };
    run_collection_operation(state, move || repository::connect_or_refresh(&url));
}

fn run_collection_operation(
    mut state: Signal<BrowserState>,
    operation: impl FnOnce() -> Result<Collection, String> + Send + 'static,
) {
    state.with_mut(|state| {
        state.loading = true;
        state.error = None;
    });
    let (sender, receiver) = futures_channel::oneshot::channel();
    std::thread::spawn(move || { let _ = sender.send(operation()); });
    spawn(async move {
        match receiver.await {
            Ok(Ok(collection)) => state.with_mut(|state| {
                state.collection = Some(collection);
                state.repository_input = state.collection.as_ref().unwrap().url.clone();
                state.selected_recipe = None;
                state.loading = false;
                state.stale = false;
                state.error = None;
            }),
            Ok(Err(error)) => state.with_mut(|state| {
                state.loading = false;
                state.stale = state.collection.is_some();
                state.error = Some(error);
            }),
            Err(_) => state.with_mut(|state| {
                state.loading = false;
                state.stale = state.collection.is_some();
                state.error = Some("The repository operation could not be completed.".to_owned());
            }),
        }
    });
}

fn disconnect_collection(mut state: Signal<BrowserState>) {
    let Some(url) = state.read().collection.as_ref().map(|collection| collection.url.clone()) else {
        return;
    };
    state.with_mut(|state| { state.loading = true; state.error = None; });
    let (sender, receiver) = futures_channel::oneshot::channel();
    std::thread::spawn(move || { let _ = sender.send(repository::disconnect(&url)); });
    spawn(async move {
        match receiver.await {
            Ok(Ok(())) => state.set(BrowserState::default()),
            Ok(Err(error)) => state.with_mut(|state| {
                state.loading = false;
                state.error = Some(error);
            }),
            Err(_) => state.with_mut(|state| {
                state.loading = false;
                state.error = Some("The repository could not be disconnected.".to_owned());
            }),
        }
    });
}

fn recipe_tags(recipe: &RecipeFile) -> Option<Vec<String>> {
    recipe.recipe.as_ref().ok()?.metadata.tags().map(|tags| tags.into_iter().map(|tag| tag.into_owned()).collect())
}

fn recipe_time(recipe: &RecipeFile) -> Option<u32> {
    recipe.recipe.as_ref().ok()?.metadata.time(&cooklang::Converter::default()).map(|time| time.total())
}
