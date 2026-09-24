//! One module per PiKVM API category. Each exposes an accessor on [`PikvmClient`], e.g.
//! `client.atx().power(PowerAction::On, true).await`.

pub mod atx;
pub mod auth;
pub mod gpio;
pub mod hid;
pub mod misc;
pub mod msd;
pub mod redfish;
pub mod streamer;
pub mod switch;
pub mod system;

use crate::client::PikvmClient;

macro_rules! accessor {
    ($name:ident, $module:ident :: $ty:ident) => {
        impl PikvmClient {
            pub fn $name(&self) -> $module::$ty<'_> {
                $module::$ty(self)
            }
        }
    };
}

accessor!(auth, auth::AuthApi);
accessor!(system, system::SystemApi);
accessor!(hid, hid::HidApi);
accessor!(atx, atx::AtxApi);
accessor!(msd, msd::MsdApi);
accessor!(gpio, gpio::GpioApi);
accessor!(streamer, streamer::StreamerApi);
accessor!(switch, switch::SwitchApi);
accessor!(redfish, redfish::RedfishApi);
accessor!(misc, misc::MiscApi);
