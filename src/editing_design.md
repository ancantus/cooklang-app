# Editable Fields Implementation Design (with Auto-Time Updates)

## Overview
Add inline editable UI components for ingredient quantities and time values in recipes, including all timers. Changes automatically update prep/cook/total times based on timer durations.

## Enhanced Features
- **Editable Timers**: Each timer entry can be edited (like other fields in `timers.rs`)
- **Auto Recalculation**: Prep/Cook/Total times automatically update when any timer changes
- **Visual Feedback**: Show unsaved changes with indicator dot

## Architecture

### State Management (in `ui.rs`)

#### 1. EditableIngredient struct (unchanged from basic design)
```rust
#[derive(Clone)]
pub struct EditableIngredient {
    /// Original text from recipe file (for round-trip)
    pub original: String,
    /// State for the currently displayed quantity string
    pub quantity: Option<State<String>>,
}

impl EditableIngredient {
    fn from_ingredient(ing: &Ingredient, converter: &Converter) -> Self {
        let original = if let Some(s) = &ing.quantity {
            s.as_ref().to_owned()
        } else {
            String::new()
        };

        Self {
            original,
            quantity: None, // Will be populated when we need it editable
        }
    }
}
```

#### 2. EditableRecipe struct (with all timers)
```rust
#[derive(Clone)]
pub struct EditableRecipe {
    /// Original parsed recipe from cooklang parser
    pub recipe: Recipe,
    /// Path to the recipe file (for identification in UI)
    pub path: PathBuf,
    
    // Ingredient quantities (editable)
    pub ingredients: Vec<EditableIngredient>,
    
    // Time metadata states (auto-calculated from timers)
    pub prep_time: Option<State<String>>,      // e.g., "10 min"
    pub cook_time: Option<State<String>>,       // e.g., "25 min"  
    pub total_time: Option<State<String>>,      // Auto-updated from all timers
    
    // Individual timer states for editing (similar to render_timer_item but editable)
    pub timers: Vec<EditableTimer>,
    
    /// Whether this recipe has unsaved edits
    pub has_unsaved_changes: bool,
}

impl EditableRecipe {
    fn from_recipe_file(recipe_file: &RecipeFile, converter: &Converter) -> Self {
        let recipe = recipe_file.recipe.as_ref().expect("recipe should be parsed");
        
        // Convert time metadata to editable states (or auto-calculate if missing)
        let prep_time_str = recipe.metadata.get(StdKey::PrepTime).and_then(|v| v.to_string());
        let cook_time_str = recipe.metadata.get(StdKey::CookTime).and_then(|v| v.to_string());
        
        // Collect timers into editable form
        let mut timers: Vec<EditableTimer> = Vec::new();
        for (idx, timer) in recipe.timers.iter().enumerate() {
            let quantity_str = timer.quantity.as_ref().map(|s| s.as_ref().to_owned()).unwrap_or_default();
            
            // Auto-calculate prep/cook/total if not explicitly set
            let has_prep = recipe.metadata.has(StdKey::PrepTime);
            let has_cook = recipe.metadata.has(StdKey::CookTime);
            
            timers.push(EditableTimer {
                name: timer.name.clone(),
                quantity: State::local(quantity_str),
                index: idx,
            });
        }
        
        // Auto-initialize times from metadata or timers
        let prep_time = has_prep.then(|| State::local(prep_time_str.unwrap_or_default()));
        let cook_time = has_cook.then(|| State::local(cook_time_str.unwrap_or_default()));
        let total_time = if recipe.metadata.has(StdKey::TotalTime) {
            Some(State::local(recipe.metadata.get(StdKey::TotalTime).and_then(|v| v.to_string()).unwrap_or_default()))
        } else {
            None // Will be auto-calculated from timers when rendered
        };
        
        Self {
            recipe: recipe.clone(),
            path: recipe_file.path.clone(),
            ingredients: recipe.ingredients.iter().map(|ing| EditableIngredient::from_ingredient(ing, converter)).collect(),
            prep_time,
            cook_time,
            total_time,
            timers,
            has_unsaved_changes: false,
        }
    }
}

/// A timer entry with editable duration.
#[derive(Clone)]
pub struct EditableTimer {
    /// Timer display name (from recipe.timers[*].name)
    pub name: Option<String>,
    /// Duration as string (e.g., "5min" or from quantity.as_ref())
    pub quantity: State<String>,
    /// Index in original timer list (for reference during edits)
    pub index: usize,
}
```

### UI Components (in `ui.rs`)

#### 3. EditableIngredient component
```rust
#[component]
fn render_editable_ingredient(
    ingredient: &EditableIngredient,
) -> Element {
    // Extract quantity display from state
    let qty_display = if let Some(s) = &ingredient.quantity {
        s.to_string()
    } else {
        "".to_owned()
    };

    rsx! {
        span { class: "ingredient-quantity", "{qty_display}" }
    }
}
```

#### 4. EditableTimer component (NEW - inline editable duration)
```rust
#[component]
fn render_editable_timer(
    timer: &EditableTimer,
    on_change: EventHandler<MouseEvent>,
) -> Element {
    // Display shows the timer name or generic "Timer"
    // The editable quantity input is shown inline

    let Some(name) = &timer.name else {
        return rsx! { span { class: "edit-mode-active", "⏰ Timer" } };
    };

    rsx! {
        div { class: if timer.quantity.to_string().is_empty() { "edit-mode-active" } else { "edit-mode-inactive" },
            span { "{name}" }
            
            // Inline editable duration field
            input {
                class: "timer-duration-input",
                placeholder: "5min",
                value: &timer.quantity,
                oninput: move |e| timer.quantity.on_input(e),
                onblur: move |_| { 
                    // Validate and reformat if needed
                }
            }
        }
    }
}
```

#### 5. Time display component (auto-calculated from timers)
```rust
#[component]
fn render_time_display(
    prep_time: Option<&State<String>>,
    cook_time: Option<&State<String>>,
    total_time: Option<&State<String>>,
    should_auto_calc_total: bool,
) -> Element {
    // Auto-calculate total from timers if requested
    let calculated_total = if should_auto_calc_total {
        // Sum all timer durations (need to parse each editable_timer.quantity)
        Some("Auto-calculated".to_owned())
    } else {
        None
    };

    rsx! {
        div { class: "metadata-strip",
            if let Some(time) = prep_time { span { "Prep {time}" } }
            if let Some(time) = cook_time { span { "Cook {time}" } }
            if let Some(time) = calculated_total || let Some(time) = total_time { 
                span { class: "metadata-total", "Total {time}" } 
            }
        }
    }
}
```

### Helper Functions

#### Auto-calculate times from timers
```rust
fn calculate_times_from_timers(timers: &[EditableTimer]) -> (f64, f64, f64) {
    // Parse each timer's quantity string and sum up durations
    // Returns (prep, cook, total) in minutes
    // Default to 0 for any that can't be parsed
}
```

#### Auto-update on timer change
```rust
fn update_times_on_timer_edit(
    timers: &mut [EditableTimer],
    prep_time: &Option<State<String>>,
    cook_time: &Option<State<String>>,
    total_time: &Option<State<String>>,
) {
    // When any timer.editable_timer.quantity changes, recalculate all times
}
```

### BrowserState extension (unchanged from basic design)
```rust
#[derive(Default)]
pub struct BrowserState {
    // ... existing fields ...
    
    /// Currently selected recipe for editing
    pub selected_recipe: Option<PathBuf>,
    
    /// Edit mode flag (whether user can edit this recipe's fields)
    pub is_edit_mode: bool,
}
```

### Integration Strategy

#### Where to modify ingredient rendering (ui.rs around line 614):
Instead of iterating directly on `parsed.ingredients`, iterate on `EditableRecipe.ingredients`:
```rust
for ingredient in editable_recipe.ingredients.iter() {
    // Render inline editable quantity if needed
}
```

#### Where to render timers (in recipe steps, lines 619-673):
Extract timer rendering from step iteration and apply EditableTimer:
```rust
// Before: for item in recipe.steps...
// After: Extract/Editables into a new structure with editable timers
```

## Component Hierarchy

```
RecipeDetail page
├── Time metadata display (auto-updates from timers)
├── Ingredients section
│   └── EditableIngredient components (inline edit)
└── Steps section  
    └── Timer items rendered as EditableTimer components
```

## User Flow Example

1. **Load recipe** → `RecipeFile` parsed into `EditableRecipe` structure
2. **User sees timers** in the steps list (already integrated)
3. **User clicks timer duration** → inline input appears
4. **User edits "5min" to "10min"** → `State<String>` updates immediately
5. **Auto-calc triggers** → Total time recalculates from all timers
6. **Visual indicator** → Small dot shows unsaved changes on recipe title
7. **Save/Cancel** → Convert back to cooklang format and persist

## Key Decisions

1. **Inline editing for ingredients**: Tap to edit, tap elsewhere to commit
2. **All timers editable**: Each timer entry becomes an EditableTimer with quantity input
3. **Auto-update logic**: Triggered immediately when timer edits change
4. **Default values**: If metadata times missing, use timer sum as default

## Next Steps
1. Implement `EditableRecipe` and `EditableTimer` structs
2. Add `render_editable_timer` component function  
3. Modify existing ingredient rendering to support EditableIngredient  
4. Implement auto-time recalculation helper
5. Integrate into RecipeDetail component
6. Add visual indicators for unsaved changes

## Architecture

### State Management (in `ui.rs`)

#### 1. EditableIngredient struct
```rust
#[derive(Clone)]
pub struct EditableIngredient {
    // Original cooklang string representation (for round-trip)
    pub original: String,
    
    // Currently displayed quantity (can be edited)
    pub quantity: Option<State<String>>,
    
    // Whether this ingredient is currently being edited
    pub editing: bool,
}
```

#### 2. EditableRecipe struct  
```rust
#[derive(Clone)]
pub struct EditableRecipe {
    // Original Recipe from cooklang parser
    pub recipe: Recipe,
    // Path to the recipe file (for identification)
    pub path: PathBuf,
    
    // State for each ingredient quantity
    pub ingredients: Vec<EditableIngredient>,
    
    // Time metadata states
    pub prep_time: Option<State<String>>,
    pub cook_time: Option<State<String>>,
    pub total_time: Option<State<String>>,
    
    // Pending unsaved changes indicator
    pub has_unsaved_changes: bool,
}
```

### UI Components (in `ui.rs`)

#### 3. EditableIngredient component
```rust
fn render_editable_ingredient(
    ingredient_idx: usize,
    original: &str,
    quantity: Option<&State<String>>,
    editing: bool,
) -> Element {
    // When editing=true, show input field
    // Otherwise show read-only label with formatted quantity
    
    rsx! {
        div {
            class: if editing { "editable-ingredient-active" } else { "editable-ingredient-inactive" },
            
            if let Some(qty) = quantity {
                if editing {
                    input {
                        value: &qty,
                        oninput: move |e| qty.on_input(e),
                        onblur: move |_| { /* validate and format */ }
                    }
                } else {
                    text { 
                        "{qty}"  // Display formatted quantity string
                    }
                }
            } else {
                text { "No quantity" }
            }
        }
    }
}
```

#### 4. EditableTimes component
```rust
fn render_editable_times(
    prep_time: Option<&State<String>>,
    cook_time: Option<&State<String>>,
    total_time: Option<&State<String>>,
) -> Element {
    div { class: "times-container" }
        // Each time field follows same pattern as ingredient quantity
        // Convert string back to minutes for display using converter
    rsx! {
        section { class: "times-header", "Times" }
        
        if let Some(pt) = prep_time {
            div { class: "time-field" }
                label { "Prep time" }
                if pt.is_some() {
                    input {
                        placeholder: "min",
                        value: &pt,
                        oninput: |e| pt.on_input(e)
                    }
                }
            }
        }
    }
}
```

### Integration Points (in `ui.rs`)

#### 5. BrowserState extension
```rust
#[derive(Default)]
pub struct BrowserState {
    // ... existing fields ...
    
    // Currently selected recipe for editing
    pub selected_recipe: Option<PathBuf>,
    
    // Edit mode flag
    pub is_edit_mode: bool,
    
    // Collection of all editable recipes (for multi-recipe editing)
    pub editable_recipes: Vec<EditableRecipe>,
}
```

#### 6. Main app component modifications
Need to modify where ingredient and time values are rendered:
- `render_item_fragment` function for ingredients (around line 580 in ui.rs)
- Time display section (around lines 562-570)

## Conversion Functions

### String ↔ Duration/Time conversions
```rust
fn parse_duration_string(s: &str, converter: &Converter) -> Option<Duration> {
    // Parse "15min", "30 sec", etc. into Duration
}

fn format_duration(duration: Duration, converter: &Converter) -> String {
    // Convert back to cooklang format: "15min" or "15 minutes"
}

fn parse_minutes(s: &str) -> Option<f64> {
    // Extract numeric value from strings like "15 min", "30sec"
}
```

## Rendering Strategy

### For Ingredients:
- Original format preserved for round-trip
- Display shows converted, human-readable quantity
- Edit input mirrors original string format

### For Times:
- Parse from cooklang metadata (which stores as strings)
- Convert to Duration for display in UI
- Accept new string values on edit

## Example Usage Flow

1. **User loads recipe** → `RecipeFile.recipe` contains parsed Recipe
2. **Component renders** → Extract quantities/times into EditableRecipe state
3. **User edits ingredient "1cup flour"** → State changes to `"1.5 cup flour"`
4. **User taps elsewhere or saves** → Convert back to cooklang format and persist

## Next Steps
1. Implement `EditableIngredient` struct
2. Add `render_editable_ingredient` component function  
3. Modify existing ingredient rendering to use editable wrapper
4. Add similar logic for time fields
5. Test with sample recipes
