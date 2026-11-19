use std::path::PathBuf;
use std::time::Duration;

use cooklang::{Content, Converter, Item, Quantity, Recipe, metadata::StdKey};
use dioxus::prelude::*;

use cooklang_client::editing::{format_minutes, parse_value};
use cooklang_client::repository::{Collection, RecipeFile};
use cooklang_client::timers::TimerEntry;
use cooklang_client::{repository, timers};

/// One field in the recipe detail view that supports inline editing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditTarget {
    /// An amount in the ingredient list, by index into `Recipe::ingredients`.
    Ingredient(usize),
    /// A duration shown in a method step, by index into `Recipe::timers`.
    Timer(usize),
}

/// How long a press must be held before an inline editor opens.
const LONG_PRESS_MS: u64 = 450;

/// The field a press is currently arming, plus the press serial that cancels it.
///
/// A press starts a delayed task; the task only opens an editor when this still
/// holds the same serial. Lifting the finger clears it, so short taps stay short.
static LONG_PRESS: GlobalSignal<Option<(EditTarget, u64)>> = Signal::global(|| None);
/// Monotonic serial for the most recent press; prevents stale timers from firing.
static LONG_PRESS_SERIAL: GlobalSignal<u64> = Signal::global(|| 0);

/// In-memory edits for the recipe that is currently open.
///
/// The first release remains read-only on disk, so edits live here until the
/// recipe is closed and are never written back to the repository. Keeping the
/// authored values alongside the overrides lets a field return to its original
/// value without consulting the parser again.
#[derive(Clone, Default)]
struct RecipeEdits {
    authored_ingredients: Vec<Option<Quantity>>,
    authored_timers: Vec<Option<Quantity>>,
    authored_prep_minutes: Option<u32>,
    /// Edited amount per ingredient; `None` keeps the authored quantity.
    ingredients: Vec<Option<Quantity>>,
    /// Edited duration per timer; `None` keeps the authored quantity.
    timers: Vec<Option<Quantity>>,
    /// The field whose inline input is open.
    editing: Option<EditTarget>,
    /// Text currently in that input, before it is committed.
    draft: String,
}

impl RecipeEdits {
    fn new(recipe: &Recipe) -> Self {
        let converter = Converter::default();
        let authored_prep_minutes = recipe
            .metadata
            .get(StdKey::PrepTime)
            .and_then(|value| cooklang::metadata::CooklangValueExt::as_minutes(value, &converter));
        Self {
            authored_ingredients: recipe
                .ingredients
                .iter()
                .map(|ingredient| ingredient.quantity.clone())
                .collect(),
            authored_timers: recipe
                .timers
                .iter()
                .map(|timer| timer.quantity.clone())
                .collect(),
            authored_prep_minutes,
            ingredients: vec![None; recipe.ingredients.len()],
            timers: vec![None; recipe.timers.len()],
            editing: None,
            draft: String::new(),
        }
    }

    /// The amount to show for an ingredient: the edit when present, else authored.
    fn ingredient(&self, index: usize) -> Option<&Quantity> {
        self.ingredients
            .get(index)
            .and_then(Option::as_ref)
            .or_else(|| {
                self.authored_ingredients
                    .get(index)
                    .and_then(Option::as_ref)
            })
    }

    /// The duration to use for a timer: the edit when present, else authored.
    fn timer(&self, index: usize) -> Option<&Quantity> {
        self.timers
            .get(index)
            .and_then(Option::as_ref)
            .or_else(|| self.authored_timers.get(index).and_then(Option::as_ref))
    }

    fn has_changes(&self) -> bool {
        self.ingredients.iter().any(Option::is_some) || self.timers.iter().any(Option::is_some)
    }
}

/// Open the inline editor for `target` after a deliberate press.
fn arm_long_press(mut edits: Signal<RecipeEdits>, target: EditTarget, draft: String) {
    let serial = LONG_PRESS_SERIAL.with_mut(|serial| {
        *serial += 1;
        *serial
    });
    LONG_PRESS.with_mut(|press| *press = Some((target, serial)));
    spawn(async move {
        tokio::time::sleep(Duration::from_millis(LONG_PRESS_MS)).await;
        let still_pressed = LONG_PRESS
            .read()
            .is_some_and(|press| press == (target, serial));
        if still_pressed {
            cancel_long_press();
            edits.with_mut(|edits| {
                edits.editing = Some(target);
                edits.draft = draft;
            });
        }
    });
}

/// Disarm a press, either because it ended early or because editing started.
fn cancel_long_press() {
    LONG_PRESS.with_mut(|press| *press = None);
}

fn set_draft(mut edits: Signal<RecipeEdits>, value: String) {
    edits.with_mut(|edits| edits.draft = value);
}

fn cancel_edit(mut edits: Signal<RecipeEdits>) {
    edits.with_mut(|edits| {
        edits.editing = None;
        edits.draft.clear();
    });
}

/// Close the inline editor, keeping a valid value and discarding anything else.
///
/// An empty or unparsable draft leaves the previous value in place, so a stray
/// tap that opens and closes an input never erases an amount.
fn commit_edit(mut edits: Signal<RecipeEdits>) {
    let (target, draft) = {
        let snapshot = edits.read();
        let Some(target) = snapshot.editing else {
            return;
        };
        (target, snapshot.draft.trim().to_owned())
    };
    let Some(value) = parse_value(&draft) else {
        cancel_edit(edits);
        return;
    };
    edits.with_mut(|edits| {
        match target {
            EditTarget::Ingredient(index) => {
                let unit = edits
                    .authored_ingredients
                    .get(index)
                    .and_then(Option::as_ref)
                    .and_then(Quantity::unit)
                    .map(str::to_owned);
                let quantity = Quantity::new(value, unit);
                let authored = edits
                    .authored_ingredients
                    .get(index)
                    .and_then(Option::as_ref);
                if let Some(slot) = edits.ingredients.get_mut(index) {
                    *slot = (Some(&quantity) != authored).then_some(quantity);
                }
            }
            EditTarget::Timer(index) => {
                // Durations are edited as minutes, matching the timer prompt.
                let quantity = Quantity::new(value, Some("min".to_owned()));
                let authored = edits.authored_timers.get(index).and_then(Option::as_ref);
                if let Some(slot) = edits.timers.get_mut(index) {
                    *slot = (Some(&quantity) != authored).then_some(quantity);
                }
            }
        }
        edits.editing = None;
        edits.draft.clear();
    });
}

/// The text an ingredient's inline input starts with: its value, without unit.
fn ingredient_draft(quantity: Option<&Quantity>) -> String {
    quantity
        .map(|quantity| quantity.value().to_string())
        .unwrap_or_default()
}

/// The text a timer's inline input starts with, normalised to minutes.
fn timer_draft(quantity: Option<&Quantity>) -> String {
    let Some(quantity) = quantity else {
        return String::new();
    };
    let Some(total_secs) = timers::timer_duration_secs(Some(quantity)) else {
        // Keep an unusable authored value visible so it can be replaced.
        return quantity.value().to_string();
    };
    match timers::timer_range_early_secs(Some(quantity)) {
        Some(early_secs) => format!(
            "{}-{}",
            format_minutes(early_secs),
            format_minutes(total_secs)
        ),
        None => format_minutes(total_secs),
    }
}

/// Times recalculated from edited timers, in minutes: prep plus timer total.
///
/// Only timer edits trigger a recalculation. Prep time cannot be derived from
/// timers, so it stays as authored; cook becomes the sum of every timer and the
/// total is prep plus cook. `None` means "keep showing the authored metadata".
fn auto_times(edits: &RecipeEdits) -> Option<(u32, u32, u32)> {
    if !edits.timers.iter().any(Option::is_some) {
        return None;
    }
    let cook_secs: u64 = (0..edits.authored_timers.len())
        .filter_map(|index| timers::timer_duration_secs(edits.timer(index)))
        .sum();
    if cook_secs == 0 {
        return None;
    }
    let prep = edits.authored_prep_minutes.unwrap_or(0);
    let cook = u32::try_from(cook_secs.div_ceil(60)).unwrap_or(u32::MAX);
    Some((prep, cook, prep.saturating_add(cook)))
}

/// The ingredient amount, rendered as an inline editor once long-pressed.
fn render_ingredient_quantity(edits: Signal<RecipeEdits>, index: usize) -> Element {
    let quantity = edits.read().ingredient(index).cloned();
    let unit = quantity
        .as_ref()
        .and_then(Quantity::unit)
        .unwrap_or_default()
        .to_owned();
    let amount = quantity
        .as_ref()
        .map(|quantity| quantity.value().to_string())
        .unwrap_or_default();
    let (is_editing, draft) = {
        let snapshot = edits.read();
        (
            snapshot.editing == Some(EditTarget::Ingredient(index)),
            snapshot.draft.clone(),
        )
    };

    if is_editing {
        return rsx! {
            span { class: "ingredient-quantity ingredient-quantity-editing",
                input {
                    class: "inline-edit-input",
                    r#type: "text",
                    inputmode: "decimal",
                    autocapitalize: "off",
                    spellcheck: false,
                    enterkeyhint: "done",
                    value: draft,
                    oninput: move |event| set_draft(edits, event.value()),
                    onkeydown: move |event| match event.key() {
                        Key::Enter => {
                            event.prevent_default();
                            commit_edit(edits);
                        }
                        Key::Escape => cancel_edit(edits),
                        _ => {}
                    },
                    onblur: move |_| commit_edit(edits),
                    // `autofocus` is unreliable for inputs added after load;
                    // focusing explicitly also raises the on-screen keyboard.
                    onmounted: move |event| {
                        spawn(async move {
                            let _ = event.set_focus(true).await;
                        });
                    },
                    autofocus: true,
                }
                if !unit.is_empty() {
                    span { class: "ingredient-unit", "{unit}" }
                }
            }
        };
    }

    let draft = ingredient_draft(quantity.as_ref());
    rsx! {
        span { class: "ingredient-quantity",
            span {
                class: if quantity.is_some() { "editable-amount" } else { "editable-amount editable-amount-missing" },
                onpointerdown: move |_| arm_long_press(edits, EditTarget::Ingredient(index), draft.clone()),
                onpointerup: move |_| cancel_long_press(),
                onpointercancel: move |_| cancel_long_press(),
                onpointerleave: move |_| cancel_long_press(),
                oncontextmenu: move |event| event.prevent_default(),
                if quantity.is_some() { "{amount}" } else { "+" }
            }
            if !unit.is_empty() {
                span { class: "ingredient-unit", "{unit}" }
            }
        }
    }
}

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

// --- Recipe timers -----------------------------------------------------------
//
// Timers are global so they keep counting across screens and outlive the
// recipe they were started from. The tick signal is bumped while any timer is
// counting down; components read it to keep their countdowns live. All state
// is wall-clock based, so backgrounding the app and coming back later leaves
// every countdown correct.

/// Timers started from any recipe, persisted so an app kill loses nothing.
static ACTIVE_TIMERS: GlobalSignal<Vec<TimerEntry>> = Signal::global(load_active_timers);
/// Bumped while timers are counting down to re-render live countdowns.
static TIMER_TICK: GlobalSignal<u64> = Signal::global(|| 0u64);
/// A timer tapped in a step that has no authored (or usable) duration.
static TIMER_PROMPT: GlobalSignal<Option<TimerPrompt>> = Signal::global(|| None);

#[derive(Clone, PartialEq)]
struct TimerPrompt {
    id: String,
    recipe_title: String,
    label: String,
}

fn load_active_timers() -> Vec<TimerEntry> {
    let mut entries = timers::load_timers();
    let saved_count = entries.len();
    entries.retain(|entry| !entry.notified);
    if entries.len() != saved_count {
        if let Err(error) = timers::save_timers(&entries) {
            eprintln!("Removing announced timers from storage failed: {error}");
        }
    }
    entries
}

fn persist_timers() {
    let snapshot = ACTIVE_TIMERS.read().clone();
    if let Err(error) = timers::save_timers(&snapshot) {
        eprintln!("Saving timers failed: {error}");
    }
}

fn set_timer_prompt(prompt: Option<TimerPrompt>) {
    TIMER_PROMPT.with_mut(|current| *current = prompt);
}

/// Start (or restart) a timer with a fresh countdown of `total_secs`.
fn timer_start(
    id: String,
    recipe_title: String,
    label: String,
    total_secs: u64,
    range_early_at_secs: Option<u64>,
) {
    let now = timers::now_ms();
    ACTIVE_TIMERS.with_mut(|list| {
        if let Some(entry) = list.iter_mut().find(|entry| entry.id == id) {
            *entry = TimerEntry::start_with_range_at(
                &id,
                &recipe_title,
                &label,
                total_secs,
                range_early_at_secs,
                now,
            );
        } else {
            list.push(TimerEntry::start_with_range_at(
                &id,
                &recipe_title,
                &label,
                total_secs,
                range_early_at_secs,
                now,
            ));
        }
    });
    persist_timers();
}

/// Pause a counting timer or resume a paused one.
fn timer_toggle_pause(id: String) {
    let now = timers::now_ms();
    ACTIVE_TIMERS.with_mut(|list| {
        if let Some(entry) = list.iter_mut().find(|entry| entry.id == id) {
            match entry.deadline_ms {
                Some(deadline) if deadline > now => {
                    entry.paused_remaining_secs = entry.remaining_secs(now);
                    entry.deadline_ms = None;
                    entry.paused_range_remaining_secs = if entry.range_early_notified {
                        None
                    } else {
                        entry.range_early_deadline_ms.map(|deadline| {
                            u64::try_from(deadline.saturating_sub(now).div_ceil(1000)).unwrap_or(0)
                        })
                    };
                    entry.range_early_deadline_ms = None;
                }
                // Finished timers stay finished; pausing would hide that.
                Some(_) => {}
                None => {
                    if entry.paused_remaining_secs > 0 {
                        entry.deadline_ms =
                            Some(now + u128::from(entry.paused_remaining_secs) * 1000);
                        entry.paused_remaining_secs = 0;
                        if !entry.range_early_notified {
                            entry.range_early_deadline_ms = entry
                                .paused_range_remaining_secs
                                .take()
                                .map(|secs| now + u128::from(secs) * 1000);
                        }
                    }
                }
            }
        }
    });
    persist_timers();
}

/// Give a timer one more minute: extends a counting countdown, wakes a paused
/// one by a minute, or restarts a finished one for sixty seconds.
fn timer_add_minute(id: String) {
    let now = timers::now_ms();
    ACTIVE_TIMERS.with_mut(|list| {
        if let Some(entry) = list.iter_mut().find(|entry| entry.id == id) {
            match entry.deadline_ms {
                Some(deadline) if deadline > now => {
                    entry.deadline_ms = Some(deadline + 60_000);
                    entry.total_secs += 60;
                }
                Some(_) => {
                    entry.deadline_ms = Some(now + 60_000);
                    entry.total_secs = 60;
                    entry.range_early_at_secs = None;
                    entry.range_early_deadline_ms = None;
                    entry.paused_range_remaining_secs = None;
                    entry.range_early_notified = false;
                    entry.notified = false;
                }
                None => {
                    entry.paused_remaining_secs += 60;
                    entry.total_secs += 60;
                }
            }
        }
    });
    persist_timers();
}

/// Reset a timer to its full authored duration and start it again.
fn timer_restart(id: String) {
    let now = timers::now_ms();
    ACTIVE_TIMERS.with_mut(|list| {
        if let Some(entry) = list.iter_mut().find(|entry| entry.id == id) {
            entry.deadline_ms = Some(now + u128::from(entry.total_secs) * 1000);
            entry.paused_remaining_secs = 0;
            entry.range_early_deadline_ms = entry
                .range_early_at_secs
                .map(|secs| now + u128::from(secs) * 1000);
            entry.paused_range_remaining_secs = None;
            entry.range_early_notified = false;
            entry.notified = false;
        }
    });
    persist_timers();
}

/// Remove a timer entirely.
fn timer_cancel(id: String) {
    ACTIVE_TIMERS.with_mut(|list| list.retain(|entry| entry.id != id));
    persist_timers();
}

/// Keep countdowns ticking and announce timers that finish while open.
///
/// The app is suspended between ticks, so polling at 500 ms only matters while
/// a countdown is live; otherwise the loop idles at 2 s and still catches a
/// timer finishing (for the completion beep) without draining the battery.
fn start_timer_ticker() {
    use_effect(move || {
        dioxus::core::spawn_forever(async move {
            loop {
                let now = timers::now_ms();
                let (early_chimes, final_chimes) =
                    ACTIVE_TIMERS
                        .read()
                        .iter()
                        .fold((0, 0), |(early, final_count), entry| {
                            (
                                early + if entry.is_range_early_due(now) { 1 } else { 0 },
                                final_count
                                    + if entry.is_finished(now) && !entry.notified {
                                        1
                                    } else {
                                        0
                                    },
                            )
                        });
                let chimes = early_chimes + final_chimes;
                if chimes > 0 {
                    announce_finished_timer(chimes);
                    ACTIVE_TIMERS.with_mut(|entries| {
                        for entry in entries.iter_mut() {
                            if entry.is_range_early_due(now) {
                                entry.range_early_notified = true;
                                entry.range_early_deadline_ms = None;
                                entry.paused_range_remaining_secs = None;
                            }
                            if entry.is_finished(now) {
                                entry.notified = true;
                            }
                        }
                    });
                    persist_timers();
                }
                let counting = ACTIVE_TIMERS
                    .read()
                    .iter()
                    .any(|entry| entry.is_counting(now));
                if counting {
                    *TIMER_TICK.write() += 1;
                }
                tokio::time::sleep(if counting {
                    Duration::from_millis(500)
                } else {
                    Duration::from_secs(2)
                })
                .await;
            }
        });
    });
}

/// Best-effort completion beep through the WebView; silent when blocked.
fn announce_finished_timer(chimes: usize) {
    let script = r#"(function () {
            try {
                var Context = window.AudioContext || window.webkitAudioContext;
                if (!Context) { return; }
                var context = new Context();
                var oscillator = context.createOscillator();
                var gain = context.createGain();
                oscillator.type = "sine";
                oscillator.frequency.value = 880;
                oscillator.connect(gain);
                gain.connect(context.destination);
                var chimes = __CHIMES__;
                for (var i = 0; i < chimes; i++) {
                    var start = context.currentTime + i * 0.9;
                    gain.gain.setValueAtTime(0, start);
                    gain.gain.linearRampToValueAtTime(0.25, start + 0.03);
                    gain.gain.setValueAtTime(0.25, start + 0.35);
                    gain.gain.linearRampToValueAtTime(0, start + 0.48);
                }
                oscillator.start();
                setTimeout(function () {
                    try { oscillator.stop(); context.close(); } catch (error) {}
                }, chimes * 900);
            } catch (error) {}
        })();"#
        .replace("__CHIMES__", &chimes.to_string());
    dioxus::document::eval(&script);
}

#[component]
pub fn App() -> Element {
    let mut state = use_signal(BrowserState::load);
    let snapshot = state.read();
    start_timer_ticker();
    let prompt = TIMER_PROMPT.read().clone();

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
                        // A new recipe starts from its own edit state.
                        key: "{selected_path.display()}",
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

        if let Some(prompt) = prompt {
            TimerPromptModal { key: "{prompt.id}", prompt }
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
                    r#type: "text",
                    inputmode: "url",
                    autocapitalize: "off",
                    spellcheck: false,
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
    let recipes = collection
        .recipes
        .iter()
        .filter(|recipe| {
            query.is_empty()
                || recipe.title.to_lowercase().contains(&query)
                || recipe_tags(recipe)
                    .as_ref()
                    .is_some_and(|tags| tags.iter().any(|tag| tag.to_lowercase().contains(&query)))
        })
        .map(|recipe| {
            let duplicate_title = collection
                .recipes
                .iter()
                .filter(|candidate| candidate.title.eq_ignore_ascii_case(&recipe.title))
                .count()
                > 1;
            (recipe.clone(), duplicate_title, recipe.path.clone())
        })
        .collect::<Vec<_>>();

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
fn RecipeRow(
    recipe: RecipeFile,
    duplicate_title: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
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
    // Called before the early return so a recipe that fails to parse still
    // runs the same hooks. The component is keyed by recipe path, so switching
    // recipes starts from a fresh edit state.
    let initial_edits = recipe
        .as_ref()
        .and_then(|file| file.recipe.as_ref().ok())
        .map(RecipeEdits::new)
        .unwrap_or_default();
    let edits = use_signal(|| initial_edits);

    let Some(recipe_file) = recipe else {
        return rsx! {
            button { id: "recipe-back-button", class: "quiet-button back-button", onclick: move |event| on_back.call(event), "‹ Back to recipes" }
            EmptyState { title: "Recipe unavailable", detail: "Go back to the collection and open another recipe." }
        };
    };

    let parsed = recipe_file.recipe.as_ref().ok();
    // Reading both signals subscribes the detail view to timer mutations and
    // to the half-second tick, keeping inline countdowns and chips live.
    let _tick = TIMER_TICK.read();
    let timers_snapshot = ACTIVE_TIMERS.read().clone();
    let has_changes = edits.read().has_changes();
    let recalculated = auto_times(&edits.read());
    let has_editable_fields =
        parsed.is_some_and(|recipe| !recipe.ingredients.is_empty() || !recipe.timers.is_empty());
    let finished_timers = timers_snapshot
        .iter()
        .filter(|entry| entry.is_finished(timers::now_ms()))
        .cloned()
        .collect::<Vec<_>>();
    let recipe_path = recipe_file.path.display().to_string();
    let recipe_title = recipe_file.title.clone();

    let (tags, description, servings, prep_time, cook_time, total_time) = parsed
        .map(|recipe| {
            let converter = Converter::default();
            (
                recipe.metadata.tags().map(|tags| {
                    tags.into_iter()
                        .map(|tag| tag.into_owned())
                        .collect::<Vec<_>>()
                }),
                recipe.metadata.description().map(str::to_owned),
                recipe.metadata.servings().map(|value| value.to_string()),
                recipe.metadata.get(StdKey::PrepTime).and_then(|value| {
                    cooklang::metadata::CooklangValueExt::as_minutes(value, &converter)
                }),
                recipe.metadata.get(StdKey::CookTime).and_then(|value| {
                    cooklang::metadata::CooklangValueExt::as_minutes(value, &converter)
                }),
                recipe.metadata.time(&converter).map(|value| value.total()),
            )
        })
        .unwrap_or_default();

    rsx! {
        button { id: "recipe-back-button", class: "quiet-button back-button", onclick: move |event| on_back.call(event), "‹ Back to recipes" }
        section { class: "detail-page",
            header { class: "detail-heading",
                p { class: "section-label", "RECIPE" }
                h2 {
                    "{recipe_file.title}"
                    if has_changes {
                        span { class: "unsaved-dot", title: "Unsaved changes" }
                    }
                }
                p { class: "recipe-file-path", "{recipe_file.path.display()}" }
                if let Some(tags) = tags {
                    div { class: "tag-list detail-tags",
                        for tag in tags { span { class: "tag", "{tag}" } }
                    }
                }
                if let Some(description) = description {
                    p { class: "recipe-description", "{description}" }
                }
                div { class: "metadata-strip",
                    if let Some(servings) = servings { span { "Serves {servings}" } }
                    if let Some((prep, cook, total)) = recalculated {
                        if prep > 0 { span { "Prep {prep} min" } }
                        span { "Cook {cook} min" }
                        span { class: "metadata-total", "Total {total} min" }
                        span { class: "auto-tag", "auto from timers" }
                    } else {
                        if let Some(time) = prep_time { span { "Prep {time} min" } }
                        if let Some(time) = cook_time { span { "Cook {time} min" } }
                        if let Some(time) = total_time { span { class: "metadata-total", "Total {time} min" } }
                    }
                }
                if has_editable_fields {
                    p { class: "edit-hint", "Long-press a highlighted amount or timer to edit it." }
                }
            }

            // Finished timers announce themselves until they are dismissed.
            for entry in finished_timers {
                div { class: "notice notice-done",
                    span { class: "done-bell", "⏰" }
                    div {
                        p { "“{entry.label}” ({entry.recipe_title}) finished." }
                        button { class: "quiet-button notice-done-dismiss", onclick: move |_| timer_cancel(entry.id.clone()), "Dismiss" }
                    }
                }
            }

            if let Some(parsed) = parsed {
                if !parsed.ingredients.is_empty() {
                    section { class: "detail-section ingredients-section",
                        h3 { "Ingredients" }
                        ul { class: "ingredient-list",
                            for (index, ingredient) in parsed.ingredients.iter().enumerate().filter(|(_, ingredient)| ingredient.modifiers().should_be_listed()) {
                                li {
                                    {render_ingredient_quantity(edits, index)}
                                    span { class: "ingredient-name", "{ingredient.display_name()}" }
                                    if let Some(note) = ingredient.note.as_ref() { span { class: "ingredient-note", "{note}" } }
                                    if ingredient.modifiers().is_optional() { span { class: "optional-label", "optional" } }
                                }
                            }
                        }
                    }
                }

                section { class: "detail-section method-section",
                    h3 { "Method" }
                    for section in parsed.sections.iter() {
                        if let Some(name) = section.name.as_ref() { h4 { class: "method-subheading", "{name}" } }
                        ol { class: "method-steps",
                            for content in section.content.iter() {
                                if let Content::Step(step) = content {
                                    li { class: "method-step",
                                        for item in step.items.iter() {
                                            {render_step_item(parsed, &recipe_path, &recipe_title, item, &timers_snapshot, edits)}
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                for section in parsed.sections.iter() {
                    for content in section.content.iter() {
                        if let Content::Text(text) = content {
                            aside { class: "recipe-note",
                                span { class: "note-label", "NOTE" }
                                p { "{text}" }
                            }
                        }
                    }
                }
            } else {
                if let Err(error) = &recipe_file.recipe {
                    div { class: "notice notice-error", role: "alert",
                        strong { "This recipe could not be fully parsed." }
                        p { "{error}" }
                        p { "The original source is shown below so no authored text is lost." }
                    }
                }
                pre { class: "source-fallback", "{recipe_file.source}" }
            }
        }

        TimerBar {}
    }
}

/// Render one step item as text or, for timers, as a tappable pill.
///
/// Tapping starts the timer with its current duration; a timer without a usable
/// duration opens the duration prompt instead. While a timer is counting or
/// paused, tapping does nothing — its controls live on the chip in the timer
/// bar. Tapping a finished timer restarts it. Long-pressing a timer opens an
/// inline duration editor instead of starting it.
fn render_step_item(
    recipe: &Recipe,
    recipe_path: &str,
    recipe_title: &str,
    item: &Item,
    timers_snapshot: &[TimerEntry],
    edits: Signal<RecipeEdits>,
) -> Element {
    match item {
        Item::Timer { index } => {
            // Copy the index out of the reference so event handlers can own it.
            let timer_index = *index;
            let Some(timer) = recipe.timers.get(timer_index) else {
                return rsx! {};
            };
            let id = format!("{recipe_path}#{timer_index}");
            let label = timer.name.clone().unwrap_or_else(|| "Timer".to_owned());
            let recipe_title = recipe_title.to_owned();
            let quantity = edits.read().timer(timer_index).cloned();
            let authored_secs = timers::timer_duration_secs(quantity.as_ref());
            let range_early_secs = timers::timer_range_early_secs(quantity.as_ref());
            let (is_editing, draft) = {
                let snapshot = edits.read();
                (
                    snapshot.editing == Some(EditTarget::Timer(timer_index)),
                    snapshot.draft.clone(),
                )
            };
            if is_editing {
                return rsx! {
                    span { class: "inline-timer inline-timer-editing",
                        "⏱ {label} ·"
                        input {
                            class: "inline-timer-input",
                            r#type: "text",
                            inputmode: "decimal",
                            autocapitalize: "off",
                            spellcheck: false,
                            enterkeyhint: "done",
                            value: draft,
                            oninput: move |event| set_draft(edits, event.value()),
                            onkeydown: move |event| match event.key() {
                                Key::Enter => {
                                    event.prevent_default();
                                    commit_edit(edits);
                                }
                                Key::Escape => cancel_edit(edits),
                                _ => {}
                            },
                            onblur: move |_| commit_edit(edits),
                            onmounted: move |event| {
                                spawn(async move {
                                    let _ = event.set_focus(true).await;
                                });
                            },
                            autofocus: true,
                        }
                        span { "min" }
                    }
                };
            }

            let entry = timers_snapshot.iter().find(|entry| entry.id == id).cloned();
            let now = timers::now_ms();
            let (display, state_class) = match &entry {
                Some(entry) if entry.is_finished(now) => ("done".to_owned(), "inline-timer-done"),
                Some(entry) if entry.is_paused() => (
                    timers::format_countdown(entry.paused_remaining_secs),
                    "inline-timer-paused",
                ),
                Some(entry) => (
                    timers::format_countdown(entry.remaining_secs(now)),
                    "inline-timer-running",
                ),
                None => (
                    authored_secs
                        .map(timers::format_countdown)
                        .unwrap_or_else(|| "set time".to_owned()),
                    "inline-timer-idle",
                ),
            };
            let draft = timer_draft(quantity.as_ref());
            rsx! {
                button {
                    class: "inline-timer {state_class}",
                    onpointerdown: move |_| arm_long_press(edits, EditTarget::Timer(timer_index), draft.clone()),
                    onpointerup: move |_| cancel_long_press(),
                    onpointercancel: move |_| cancel_long_press(),
                    onpointerleave: move |_| cancel_long_press(),
                    oncontextmenu: move |event| event.prevent_default(),
                    onclick: move |_| {
                        // A long press already opened the editor; the release
                        // must not also start the timer.
                        if edits.read().editing == Some(EditTarget::Timer(timer_index)) {
                            return;
                        }
                        match &entry {
                            Some(entry) if entry.is_finished(timers::now_ms()) => timer_restart(id.clone()),
                            Some(_) => {}
                            None => match authored_secs {
                                Some(secs) => timer_start(
                                    id.clone(),
                                    recipe_title.clone(),
                                    label.clone(),
                                    secs,
                                    range_early_secs,
                                ),
                                None => set_timer_prompt(Some(TimerPrompt {
                                    id: id.clone(),
                                    recipe_title: recipe_title.clone(),
                                    label: label.clone(),
                                })),
                            },
                        }
                    },
                    "⏱ {label} · {display}"
                }
            }
        }
        other => rsx! { "{render_item_fragment(recipe, other)}" },
    }
}

/// Plain-text fragment for every non-timer step item.
fn render_item_fragment(recipe: &Recipe, item: &Item) -> String {
    match item {
        Item::Text { value } => value.clone(),
        Item::Ingredient { index } => recipe
            .ingredients
            .get(*index)
            .map(|ingredient| ingredient.display_name().into_owned())
            .unwrap_or_default(),
        Item::Cookware { index } => recipe
            .cookware
            .get(*index)
            .map(|cookware| cookware.display_name().to_owned())
            .unwrap_or_default(),
        Item::InlineQuantity { index } => recipe
            .inline_quantities
            .get(*index)
            .map(ToString::to_string)
            .unwrap_or_default(),
        Item::Timer { .. } => String::new(),
    }
}

/// Docked strip of every active timer, shown while a recipe is open.
#[component]
fn TimerBar() -> Element {
    // Components do not automatically rerender when their parent reads this
    // global signal, so subscribe the dock directly to the countdown ticker.
    let _tick = TIMER_TICK.read();
    let timers_snapshot = ACTIVE_TIMERS.read().clone();
    if timers_snapshot.is_empty() {
        return rsx! {};
    }
    let now = timers::now_ms();
    rsx! {
        div { class: "timer-bar", id: "timer-bar",
            for entry in timers_snapshot {
                TimerChip { key: "{entry.id}", entry, now }
            }
        }
    }
}

/// One timer chip with its countdown and full controls.
#[component]
fn TimerChip(entry: TimerEntry, now: u128) -> Element {
    let id = entry.id.clone();
    let pause_id = id.clone();
    let add_minute_id = id.clone();
    let restart_id = id.clone();
    let cancel_id = id;
    let finished = entry.is_finished(now);
    let paused = entry.is_paused();
    let time_text = if finished {
        "✓ done".to_owned()
    } else if paused {
        timers::format_countdown(entry.paused_remaining_secs)
    } else {
        timers::format_countdown(entry.remaining_secs(now))
    };
    let state_class = if finished {
        "chip-done"
    } else if paused {
        "chip-paused"
    } else {
        "chip-running"
    };
    let range_color_class = if entry.range_early_at_secs.is_some()
        && (entry.range_early_notified || entry.is_range_early_due(now))
    {
        "timer-range-orange"
    } else {
        "timer-range-green"
    };
    let remaining_secs = if finished {
        0
    } else if paused {
        entry.paused_remaining_secs
    } else {
        entry.remaining_secs(now)
    };
    let progress = if entry.total_secs == 0 {
        100.0
    } else {
        (1.0 - remaining_secs as f64 / entry.total_secs as f64).clamp(0.0, 1.0) * 100.0
    };
    rsx! {
        div {
            class: "timer-chip {state_class} {range_color_class}",
            style: "--timer-progress: {progress}%",
            div { class: "timer-chip-main",
                span { class: "timer-chip-time", "{time_text}" }
                span { class: "timer-chip-label", "{entry.label}" }
                span { class: "timer-chip-sub", "{entry.recipe_title}" }
            }
            div { class: "timer-chip-actions",
                if !finished {
                    button {
                        class: "chip-button",
                        title: if paused { "Resume" } else { "Pause" },
                        onclick: move |_| timer_toggle_pause(pause_id.clone()),
                        if paused { "▶" } else { "⏸" }
                    }
                }
                button { class: "chip-button", title: "Add one minute", onclick: move |_| timer_add_minute(add_minute_id.clone()), "+1′" }
                button { class: "chip-button", title: "Restart", onclick: move |_| timer_restart(restart_id.clone()), "↺" }
                button { class: "chip-button", title: "Remove timer", onclick: move |_| timer_cancel(cancel_id.clone()), "✕" }
            }
        }
    }
}

/// Ask for a duration when a tapped timer has none the app can use.
#[component]
fn TimerPromptModal(prompt: TimerPrompt) -> Element {
    let mut minutes_input = use_signal(String::new);
    let entered = minutes_input
        .read()
        .trim()
        .parse::<f64>()
        .ok()
        .map(|minutes| (minutes * 60.0).round() as u64)
        .filter(|secs| *secs > 0);
    rsx! {
        div { class: "timer-modal",
            onclick: move |_| set_timer_prompt(None),
            div { class: "timer-modal-card",
                onclick: move |event| event.stop_propagation(),
                h3 { "Set “{prompt.label}”" }
                p { class: "timer-modal-hint",
                    "This timer has no duration in the recipe. How long should it run?"
                }
                input {
                    r#type: "number",
                    inputmode: "decimal",
                    min: "0",
                    step: "any",
                    placeholder: "Minutes, e.g. 10",
                    value: minutes_input.read().clone(),
                    oninput: move |event| minutes_input.set(event.value()),
                    autofocus: true,
                }
                div { class: "timer-modal-actions",
                    button { class: "quiet-button", onclick: move |_| set_timer_prompt(None), "Cancel" }
                    button {
                        class: "primary-button",
                        disabled: entered.is_none(),
                        onclick: move |_| {
                            if let Some(secs) = entered {
                                timer_start(
                                    prompt.id.clone(),
                                    prompt.recipe_title.clone(),
                                    prompt.label.clone(),
                                    secs,
                                    None,
                                );
                                set_timer_prompt(None);
                            }
                        },
                        "Start timer"
                    }
                }
            }
        }
    }
}

fn connect_collection(state: Signal<BrowserState>) {
    let url = state.read().repository_input.clone();
    run_collection_operation(state, move || repository::connect_or_refresh(&url));
}

fn refresh_collection(state: Signal<BrowserState>) {
    let Some(url) = state
        .read()
        .collection
        .as_ref()
        .map(|collection| collection.url.clone())
    else {
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
    std::thread::spawn(move || {
        let _ = sender.send(operation());
    });
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
    let Some(url) = state
        .read()
        .collection
        .as_ref()
        .map(|collection| collection.url.clone())
    else {
        return;
    };
    state.with_mut(|state| {
        state.loading = true;
        state.error = None;
    });
    let (sender, receiver) = futures_channel::oneshot::channel();
    std::thread::spawn(move || {
        let _ = sender.send(repository::disconnect(&url));
    });
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
    recipe
        .recipe
        .as_ref()
        .ok()?
        .metadata
        .tags()
        .map(|tags| tags.into_iter().map(|tag| tag.into_owned()).collect())
}

fn recipe_time(recipe: &RecipeFile) -> Option<u32> {
    recipe
        .recipe
        .as_ref()
        .ok()?
        .metadata
        .time(&cooklang::Converter::default())
        .map(|time| time.total())
}
