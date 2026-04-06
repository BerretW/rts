mod components;
mod systems;
mod defs;
mod screens;
mod scripting;
mod state;

use state::GameRoot;

fn main() {
    engine::app::run("RTS – Warcraft 2 style", 1280, 720, GameRoot::new());
}
