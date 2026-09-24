//! Streamer & OCR card (advanced).

use std::cell::Cell;

use adw::prelude::*;
use pikvm::events::KvmState;
use pikvm::models::{OcrOptions, OcrRegion, SnapshotOptions};

use super::{call, client, Card};
use crate::util::{self, spawn_result, toast};

pub struct StreamerCard {
    group: adw::PreferencesGroup,
    encoder_row: adw::ActionRow,
    source_row: adw::ActionRow,
    h264_row: adw::ActionRow,
    clients_row: adw::ActionRow,
    snapshot_row: adw::ActionRow,
    ocr_row: adw::ActionRow,
    advanced: Cell<bool>,
}

impl StreamerCard {
    pub fn new() -> Self {
        let group = util::card(
            "Streamer & OCR",
            Some("µStreamer state, snapshots and text recognition"),
        );
        let encoder_row = util::kv_row(&group, "Encoder");
        let source_row = util::kv_row(&group, "Source");
        let h264_row = util::kv_row(&group, "H.264");
        let clients_row = util::kv_row(&group, "Clients");
        let snapshot_row = util::kv_row(&group, "Saved snapshot");
        let ocr_row = util::kv_row(&group, "OCR");

        let snap = util::button("Take snapshot");
        snap.connect_clicked(|b| {
            let Some(c) = client() else { return };
            let parent = b.clone();
            spawn_result(
                "Snapshot",
                async move {
                    let opts = SnapshotOptions {
                        save: true,
                        allow_offline: true,
                        ..Default::default()
                    };
                    Ok(c.streamer().snapshot(&opts).await?)
                },
                move |jpeg| show_image_dialog(&parent, &jpeg),
            );
        });
        let delete = util::button("Delete saved");
        delete.connect_clicked(|_| {
            call(
                "Delete snapshot",
                |c| async move { c.streamer().delete_snapshot().await },
            )
        });
        let row = adw::ActionRow::builder()
            .title("Snapshot")
            .subtitle("GET/DELETE /api/streamer/snapshot")
            .build();
        row.add_suffix(&util::button_box(&[&snap, &delete]));
        group.add(&row);

        let langs = adw::EntryRow::builder()
            .title("OCR languages (comma separated)")
            .text("eng")
            .build();
        group.add(&langs);
        let region = adw::EntryRow::builder()
            .title("OCR region left,top,right,bottom (empty = full frame)")
            .build();
        group.add(&region);
        let ocr = util::suggested_button("Run OCR");
        let (l, r) = (langs.clone(), region.clone());
        ocr.connect_clicked(move |b| {
            let Some(c) = client() else { return };
            let langs: Vec<String> = l
                .text()
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            let region = parse_region(&r.text());
            let parent = b.clone();
            spawn_result(
                "OCR",
                async move {
                    let opts = OcrOptions {
                        langs,
                        region,
                        allow_offline: true,
                    };
                    Ok(c.streamer().snapshot_ocr(&opts).await?)
                },
                move |text| show_text_dialog(&parent, "Recognised text", &text),
            );
        });
        util::button_row(&group, "Text recognition", Some("Tesseract on the PiKVM"), &ocr);

        Self {
            group,
            encoder_row,
            source_row,
            h264_row,
            clients_row,
            snapshot_row,
            ocr_row,
            advanced: Cell::new(false),
        }
    }
}

fn parse_region(text: &str) -> Option<OcrRegion> {
    let nums: Vec<u32> = text.split(',').filter_map(|s| s.trim().parse().ok()).collect();
    (nums.len() == 4).then(|| OcrRegion {
        left: nums[0],
        top: nums[1],
        right: nums[2],
        bottom: nums[3],
    })
}

pub fn show_text_dialog(parent: &impl IsA<gtk::Widget>, title: &str, text: &str) {
    let dialog = adw::Dialog::builder()
        .title(title)
        .content_width(640)
        .content_height(480)
        .build();
    let toolbar = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    let copy = gtk::Button::from_icon_name("edit-copy-symbolic");
    let t = text.to_string();
    copy.connect_clicked(move |b| util::copy_to_clipboard(b, &t));
    header.pack_end(&copy);
    toolbar.add_top_bar(&header);
    let view = gtk::TextView::builder()
        .editable(false)
        .monospace(true)
        .wrap_mode(gtk::WrapMode::WordChar)
        .build();
    view.buffer().set_text(text);
    view.set_margin_top(8);
    view.set_margin_start(8);
    view.set_margin_end(8);
    let scrolled = gtk::ScrolledWindow::builder().child(&view).vexpand(true).build();
    toolbar.set_content(Some(&scrolled));
    dialog.set_child(Some(&toolbar));
    dialog.present(Some(parent));
}

pub fn show_image_dialog(parent: &impl IsA<gtk::Widget>, jpeg: &[u8]) {
    let bytes = gtk::glib::Bytes::from(jpeg);
    match gtk::gdk::Texture::from_bytes(&bytes) {
        Ok(texture) => {
            let dialog = adw::Dialog::builder()
                .title("Snapshot")
                .content_width(960)
                .content_height(600)
                .build();
            let toolbar = adw::ToolbarView::new();
            toolbar.add_top_bar(&adw::HeaderBar::new());
            let picture = gtk::Picture::for_paintable(&texture);
            picture.set_content_fit(gtk::ContentFit::Contain);
            picture.set_vexpand(true);
            toolbar.set_content(Some(&picture));
            dialog.set_child(Some(&toolbar));
            dialog.present(Some(parent));
        }
        Err(e) => toast(&format!("Snapshot could not be decoded: {e}")),
    }
}

impl Card for StreamerCard {
    fn group(&self) -> &adw::PreferencesGroup {
        &self.group
    }

    fn update(&self, s: &KvmState, connected: bool) {
        self.group.set_visible(connected && self.advanced.get());
        let st = &s.streamer;
        match &st.streamer {
            Some(r) => {
                self.encoder_row
                    .set_subtitle(&format!("{} · quality {}", r.encoder.kind, r.encoder.quality));
                self.source_row.set_subtitle(&format!(
                    "{} · {}×{} · {} fps captured (desired {})",
                    if r.source.online { "online" } else { "offline" },
                    r.source.resolution.width,
                    r.source.resolution.height,
                    r.source.captured_fps,
                    r.source.desired_fps
                ));
                self.h264_row.set_subtitle(&format!(
                    "{} · {} kbps · gop {} · {} fps",
                    if r.h264["online"].as_bool().unwrap_or(false) {
                        "online"
                    } else {
                        "offline"
                    },
                    r.h264["bitrate"],
                    r.h264["gop"],
                    r.h264["fps"]
                ));
                self.clients_row.set_subtitle(&format!(
                    "{} stream client(s) · sinks: {}",
                    r.stream.clients,
                    r.sinks
                        .as_object()
                        .map(|o| o
                            .iter()
                            .map(|(k, v)| format!("{k}={}", v["has_clients"]))
                            .collect::<Vec<_>>()
                            .join(", "))
                        .unwrap_or_default()
                ));
            }
            None => {
                self.encoder_row.set_subtitle("streamer stopped");
                self.source_row.set_subtitle("—");
                self.h264_row.set_subtitle("—");
                self.clients_row.set_subtitle("—");
            }
        }
        self.snapshot_row.set_subtitle(&match st.snapshot["saved"].as_object() {
            Some(saved) => format!("saved: {}", saved.get("pos").map(|v| v.to_string()).unwrap_or_default()),
            None => "none".into(),
        });
        self.ocr_row.set_subtitle(&format!(
            "features: {}",
            st.features
                .iter()
                .map(|(k, v)| format!("{k}={}", util::yes_no(*v)))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    fn set_advanced(&self, advanced: bool) {
        self.advanced.set(advanced);
        self.group.set_visible(advanced && self.group.is_visible());
    }
}
