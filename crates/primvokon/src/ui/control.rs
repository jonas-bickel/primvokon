//! ControlPage (stub, filled in below).

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct ControlPage;

    #[glib::object_subclass]
    impl ObjectSubclass for ControlPage {
        const NAME: &'static str = "PvControlPage";
        type Type = super::ControlPage;
        type ParentType = adw::Bin;
    }

    impl ObjectImpl for ControlPage {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().set_child(Some(&crate::util::status_page("emblem-synchronizing-symbolic", "ControlPage", "Coming soon")));
        }
    }
    impl WidgetImpl for ControlPage {}
    impl BinImpl for ControlPage {}
}

glib::wrapper! {
    pub struct ControlPage(ObjectSubclass<imp::ControlPage>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for ControlPage {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl ControlPage {
    pub fn autostart(&self) {}
}
