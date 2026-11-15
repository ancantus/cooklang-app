use dioxus::prelude::*;

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        style { {include_str!("../assets/main.css")} }
        main {
            h1 { "Cooklang" }
            p { "Your recipes, ready to cook." }
            section { class: "empty-state",
                h2 { "No recipes yet" }
                p { "Your Cooklang collection will appear here." }
            }
        }
    }
}
