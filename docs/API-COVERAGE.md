# PiKVM API coverage matrix

Source: https://docs.pikvm.org/api/ (kvmd `api.md`). Every route below is implemented in
`crates/pikvm` (`client::PikvmClient`) and described in the endpoint catalogue
(`pikvm::catalog`) that drives the API Explorer. **Importance** decides default visibility:
`core` = shown by default, `advanced` = behind *Show advanced*, `explorer` = API Explorer only.

| Category | Method | Route | Client method | Importance | UI location |
|----------|--------|-------|---------------|------------|-------------|
| Auth | POST | `/api/auth/login` | `auth::login` | core | Connection (implicit) / Explorer |
| Auth | GET | `/api/auth/check` | `auth::check` | core | Connection → Test |
| Auth | POST | `/api/auth/logout` | `auth::logout` | advanced | Profile menu → Sign out |
| WebSocket | WS | `/api/ws?stream=0|1` | `ws::WsClient` | core | Console (stream + state), background services |
| System | GET | `/api/info?fields=` | `system::info` | core | Control → System card |
| System | GET | `/api/log?follow=&seek=` | `system::log`, `system::log_follow` | advanced | Control → Log viewer |
| HID | GET | `/api/hid` | `hid::state` | core | Control → HID card |
| HID | POST | `/api/hid/set_params` | `hid::set_params` | advanced | HID card (advanced) |
| HID | POST | `/api/hid/set_connected` | `hid::set_connected` | advanced | HID card (advanced) |
| HID | POST | `/api/hid/reset` | `hid::reset` | advanced | HID card (advanced) |
| HID | GET | `/api/hid/keymaps` | `hid::keymaps` | core | Paste-text dialog |
| HID | POST | `/api/hid/print` | `hid::print` | core | Console → Paste text, HID card |
| HID | POST | `/api/hid/events/send_shortcut` | `hid::send_shortcut` | core | Console quick actions, HID shortcut palette |
| HID | POST | `/api/hid/events/send_key` | `hid::send_key` | advanced | HID card (advanced) |
| HID | POST | `/api/hid/events/send_mouse_button` | `hid::send_mouse_button` | advanced | HID card (advanced) |
| HID | POST | `/api/hid/events/send_mouse_move` | `hid::send_mouse_move` | advanced | HID card (advanced) |
| HID | POST | `/api/hid/events/send_mouse_relative` | `hid::send_mouse_relative` | advanced | HID card (advanced) |
| HID | POST | `/api/hid/events/send_mouse_wheel` | `hid::send_mouse_wheel` | advanced | HID card (advanced) |
| ATX | GET | `/api/atx` | `atx::state` | core | Control → ATX card, Console status |
| ATX | POST | `/api/atx/power?action=&wait=` | `atx::power` | core | ATX card |
| ATX | POST | `/api/atx/click?button=&wait=` | `atx::click` | core | ATX card, Console quick actions |
| MSD | GET | `/api/msd` | `msd::state` | core | Control → MSD card |
| MSD | POST | `/api/msd/write?image=` | `msd::write` | advanced | MSD card → Upload file |
| MSD | POST | `/api/msd/write_remote?url=&image=&timeout=` | `msd::write_remote` | advanced | MSD card → Upload from URL |
| MSD | POST | `/api/msd/set_params?image=&cdrom=&rw=` | `msd::set_params` | core | MSD card |
| MSD | POST | `/api/msd/set_connected?connected=` | `msd::set_connected` | core | MSD card |
| MSD | POST | `/api/msd/remove?image=` | `msd::remove` | advanced | MSD card |
| MSD | POST | `/api/msd/reset` | `msd::reset` | advanced | MSD card |
| GPIO | GET | `/api/gpio` | `gpio::state` | core | Control → GPIO card |
| GPIO | POST | `/api/gpio/switch?channel=&state=&wait=` | `gpio::switch` | core | GPIO card |
| GPIO | POST | `/api/gpio/pulse?channel=&delay=&wait=` | `gpio::pulse` | core | GPIO card |
| Streamer | GET | `/api/streamer` | `streamer::state` | advanced | Control → Streamer card |
| Streamer | GET | `/api/streamer/snapshot?…` | `streamer::snapshot`, `streamer::snapshot_ocr` | advanced | Streamer card, Watcher/Recall services |
| Streamer | DELETE | `/api/streamer/snapshot` | `streamer::delete_snapshot` | advanced | Streamer card |
| Streamer | GET | `/api/streamer/ocr` | `streamer::ocr_state` | advanced | Streamer card |
| Streamer | POST | `/api/streamer/set_params?quality=&desired_fps=&h264_bitrate=&h264_gop=` | `streamer::set_params` | advanced | Console (advanced) |
| Streamer | GET | `/streamer/stream` | `stream::MjpegStream` | core | Console video |
| Switch | GET | `/api/switch` | `switch::state` | core* | Control → Switch card |
| Switch | POST | `/api/switch/set_active_prev` | `switch::set_active_prev` | core* | Switch card |
| Switch | POST | `/api/switch/set_active_next` | `switch::set_active_next` | core* | Switch card |
| Switch | POST | `/api/switch/set_active?port=` | `switch::set_active` | core* | Switch card |
| Switch | POST | `/api/switch/set_beacon?state=&port=&uplink=&downlink=` | `switch::set_beacon` | advanced | Switch card |
| Switch | POST | `/api/switch/set_port_params?…` | `switch::set_port_params` | advanced | Switch card → Port dialog |
| Switch | POST | `/api/switch/set_colors` (body) | `switch::set_colors` | advanced | Switch card |
| Switch | POST | `/api/switch/reset?unit=&bootloader=` | `switch::reset` | advanced | Switch card |
| Switch | POST | `/api/switch/edids/create?name=&data=` | `switch::edid_create` | advanced | Switch card → EDIDs |
| Switch | POST | `/api/switch/edids/change?id=&name=&data=` | `switch::edid_change` | advanced | Switch card → EDIDs |
| Switch | POST | `/api/switch/edids/remove?id=` | `switch::edid_remove` | advanced | Switch card → EDIDs |
| Switch | POST | `/api/switch/atx/power?port=&action=` | `switch::atx_power` | advanced | Switch card |
| Switch | POST | `/api/switch/atx/click?port=&button=` | `switch::atx_click` | advanced | Switch card |
| Redfish | GET | `/api/redfish/v1` | `redfish::root` | explorer | API Explorer |
| Redfish | GET | `/api/redfish/v1/Systems` | `redfish::systems` | explorer | API Explorer |
| Redfish | GET | `/api/redfish/v1/Systems/{id}` | `redfish::system` | explorer | API Explorer |
| Redfish | PATCH | `/api/redfish/v1/Systems/{id}` | `redfish::patch_system` | explorer | API Explorer |
| Redfish | POST | `/api/redfish/v1/Systems/{id}/Actions/ComputerSystem.Reset` | `redfish::reset` | explorer | API Explorer |
| Misc | GET | `/api/export/prometheus/metrics` | `misc::prometheus_metrics` | explorer | API Explorer |

`*` core when a PiKVM Switch is detected, hidden otherwise.

WebSocket events handled: `gpio_model_state`, `gpio_state`, `hid_state`, `hid_keymaps_state`,
`atx_state`, `msd_state`, `streamer_state`, `info_*_state`, `switch_state`, `wol_state`,
`loop`, `pong`. Client → server: `ping`, `key`, `mouse_button`, `mouse_move`, `mouse_relative`,
`mouse_wheel`.
