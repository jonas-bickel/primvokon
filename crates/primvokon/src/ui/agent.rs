//! AgentPage (stub, filled in below).

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct AgentPage;

    #[glib::object_subclass]
    impl ObjectSubclass for AgentPage {
        const NAME: &'static str = "PvAgentPage";
        type Type = super::AgentPage;
        type ParentType = adw::Bin;
    }

    impl ObjectImpl for AgentPage {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().set_child(Some(&crate::util::status_page("emblem-synchronizing-symbolic", "AgentPage", "Coming soon")));
        }
    }
    impl WidgetImpl for AgentPage {}
    impl BinImpl for AgentPage {}
}

glib::wrapper! {
    pub struct AgentPage(ObjectSubclass<imp::AgentPage>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for AgentPage {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl AgentPage {
    pub fn autostart(&self) {}
}
