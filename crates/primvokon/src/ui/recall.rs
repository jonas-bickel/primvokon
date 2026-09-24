//! RecallPage (stub, filled in below).

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct RecallPage;

    #[glib::object_subclass]
    impl ObjectSubclass for RecallPage {
        const NAME: &'static str = "PvRecallPage";
        type Type = super::RecallPage;
        type ParentType = adw::Bin;
    }

    impl ObjectImpl for RecallPage {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().set_child(Some(&crate::util::status_page("emblem-synchronizing-symbolic", "RecallPage", "Coming soon")));
        }
    }
    impl WidgetImpl for RecallPage {}
    impl BinImpl for RecallPage {}
}

glib::wrapper! {
    pub struct RecallPage(ObjectSubclass<imp::RecallPage>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for RecallPage {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl RecallPage {
    pub fn autostart(&self) {}
}
