//! WatchPage (stub, filled in below).

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct WatchPage;

    #[glib::object_subclass]
    impl ObjectSubclass for WatchPage {
        const NAME: &'static str = "PvWatchPage";
        type Type = super::WatchPage;
        type ParentType = adw::Bin;
    }

    impl ObjectImpl for WatchPage {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().set_child(Some(&crate::util::status_page("emblem-synchronizing-symbolic", "WatchPage", "Coming soon")));
        }
    }
    impl WidgetImpl for WatchPage {}
    impl BinImpl for WatchPage {}
}

glib::wrapper! {
    pub struct WatchPage(ObjectSubclass<imp::WatchPage>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for WatchPage {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl WatchPage {
    pub fn autostart(&self) {}
}
