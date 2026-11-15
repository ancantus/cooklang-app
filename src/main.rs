pub mod repository;
mod ui;

fn main() {
    dioxus::launch(ui::App);
}
