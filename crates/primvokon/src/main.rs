//! PRIMVOKON – agentic remote for PiKVM.

mod app;
mod connection;
mod prefs;
mod state;
mod ui;
mod util;
mod window;

use gtk::prelude::*;
use gtk::{gio, glib};

fn main() -> glib::ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "primvokon=info,pikvm=info".into()),
        )
        .init();
    gio::resources_register_include!("primvokon.gresource").expect("register resources");
    glib::set_application_name("PRIMVOKON");
    let app = app::PvApplication::new();
    app.run()
}
