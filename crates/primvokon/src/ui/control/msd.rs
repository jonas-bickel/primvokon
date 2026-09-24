//! Mass storage drive card.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use adw::prelude::*;
use pikvm::events::KvmState;

use super::{call, client, Card};
use crate::util::{self, confirm, human_bytes, set_pill, spawn_result, toast};

pub struct MsdCard {
    group: adw::PreferencesGroup,
    status_pill: gtk::Label,
    drive_row: adw::ActionRow,
    storage_row: adw::ActionRow,
    image_combo: adw::ComboRow,
    images: Rc<RefCell<Vec<String>>>,
    media_combo: adw::ComboRow,
    rw_switch: adw::SwitchRow,
    transfer_row: adw::ActionRow,
    advanced: Vec<gtk::Widget>,
    upload_progress: gtk::ProgressBar,
}

impl MsdCard {
    pub fn new() -> Self {
        let group = util::card(
            "Mass storage drive",
            Some("Virtual CD-ROM / flash drive presented to the host"),
        );
        let status_pill = util::make_pill("offline", None);
        let status = adw::ActionRow::builder().title("Status").build();
        status.add_suffix(&status_pill);
        group.add(&status);
        let drive_row = util::kv_row(&group, "Drive");
        let storage_row = util::kv_row(&group, "Storage");
        let transfer_row = util::kv_row(&group, "Transfer");
        transfer_row.set_visible(false);

        let images: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        let image_combo = util::combo_row(&group, "Image", Some("Select the image to present"), &[], 0);
        let media_combo = util::combo_row(&group, "Media type", None, &["CD-ROM", "Flash"], 0);
        let rw_switch = util::switch_row(
            &group,
            "Read-write",
            Some("Flash only; CD-ROM is always read-only"),
            false,
        );
        let select = util::suggested_button("Apply selection");
        let connect = util::button("Connect");
        let disconnect = util::button("Disconnect");
        let (ic, mc, rw, imgs) = (
            image_combo.clone(),
            media_combo.clone(),
            rw_switch.clone(),
            images.clone(),
        );
        select.connect_clicked(move |_| {
            let image = imgs.borrow().get(ic.selected() as usize).cloned();
            let cdrom = mc.selected() == 0;
            let rw = rw.is_active();
            call("MSD parameters", move |c| async move {
                c.msd().set_params(image.as_deref(), Some(cdrom), Some(rw)).await
            });
        });
        connect.connect_clicked(|_| call("MSD connect", |c| async move { c.msd().set_connected(true).await }));
        disconnect.connect_clicked(|_| call("MSD disconnect", |c| async move { c.msd().set_connected(false).await }));
        let arow = adw::ActionRow::builder().title("Actions").build();
        arow.add_suffix(&util::button_box(&[&select, &connect, &disconnect]));
        group.add(&arow);

        let mut advanced: Vec<gtk::Widget> = Vec::new();

        // Upload from URL.
        let url_row = adw::EntryRow::builder()
            .title("Upload from URL (https://…/image.iso)")
            .build();
        let url_button = util::button("Download to PiKVM");
        url_button.set_valign(gtk::Align::Center);
        let url_ref = url_row.clone();
        url_button.connect_clicked(move |_| {
            let url = url_ref.text().to_string();
            if url.is_empty() {
                return;
            }
            let Some(c) = client() else { return };
            toast("Download started – progress is shown in the Transfer row");
            spawn_result(
                "Upload from URL",
                async move { Ok(c.msd().write_remote(&url, None, None).await?) },
                |_| toast("Download finished"),
            );
        });
        url_row.add_suffix(&url_button);
        group.add(&url_row);
        advanced.push(url_row.clone().upcast());

        // Upload local file.
        let upload_progress = gtk::ProgressBar::builder().visible(false).show_text(true).build();
        let upload_button = util::button("Choose file…");
        let progress_ref = upload_progress.clone();
        upload_button.connect_clicked(move |b| {
            let Some(c) = client() else { return };
            let dialog = gtk::FileDialog::builder().title("Upload image to the PiKVM").build();
            let window = b.root().and_downcast::<gtk::Window>();
            let progress = progress_ref.clone();
            dialog.open(window.as_ref(), gtk::gio::Cancellable::NONE, move |res| {
                let Ok(file) = res else { return };
                let Some(path) = file.path() else { return };
                progress.set_visible(true);
                progress.set_fraction(0.0);
                let (tx, rx) = async_channel::unbounded::<(u64, u64)>();
                let p2 = progress.clone();
                gtk::glib::spawn_future_local(async move {
                    while let Ok((sent, total)) = rx.recv().await {
                        if total > 0 {
                            p2.set_fraction(sent as f64 / total as f64);
                            p2.set_text(Some(&format!("{} / {}", human_bytes(sent), human_bytes(total))));
                        }
                    }
                });
                let cb: pikvm::api::msd::Progress = Arc::new(move |sent, total| {
                    let _ = tx.try_send((sent, total));
                });
                let p3 = progress.clone();
                spawn_result(
                    "Upload file",
                    async move { Ok(c.msd().write_file(&path, None, Some(cb)).await?) },
                    move |_| {
                        p3.set_visible(false);
                        toast("Image uploaded");
                    },
                );
            });
        });
        let upload_row = util::button_row(
            &group,
            "Upload local file",
            Some("Streams the file to POST /api/msd/write"),
            &upload_button,
        );
        group.add(&upload_progress);
        advanced.push(upload_row.upcast());
        advanced.push(upload_progress.clone().upcast());

        // Remove / reset.
        let remove = util::destructive_button("Remove selected image");
        let (ic, imgs) = (image_combo.clone(), images.clone());
        remove.connect_clicked(move |b| {
            let Some(image) = imgs.borrow().get(ic.selected() as usize).cloned() else {
                return;
            };
            let name = image.clone();
            confirm(
                b,
                "Remove image?",
                &format!("Delete “{name}” from the PiKVM storage."),
                "Remove",
                move || call("MSD remove", move |c| async move { c.msd().remove(&image).await }),
            );
        });
        let reset = util::destructive_button("Reset MSD");
        reset.connect_clicked(|b| {
            confirm(
                b,
                "Reset the mass storage drive?",
                "Drops the selected image and parameters.",
                "Reset",
                || call("MSD reset", |c| async move { c.msd().reset().await }),
            )
        });
        let drow = adw::ActionRow::builder().title("Maintenance").build();
        drow.add_suffix(&util::button_box(&[&remove, &reset]));
        group.add(&drow);
        advanced.push(drow.upcast());

        Self {
            group,
            status_pill,
            drive_row,
            storage_row,
            image_combo,
            images,
            media_combo,
            rw_switch,
            transfer_row,
            advanced,
            upload_progress,
        }
    }
}

impl Card for MsdCard {
    fn group(&self) -> &adw::PreferencesGroup {
        &self.group
    }

    fn update(&self, s: &KvmState, connected: bool) {
        let m = &s.msd;
        self.group.set_visible(connected && m.enabled);
        set_pill(
            &self.status_pill,
            &format!(
                "{}{}",
                if m.online { "online" } else { "offline" },
                if m.busy { " · busy" } else { "" }
            ),
            Some(if m.online { "success" } else { "warning" }),
        );
        self.drive_row.set_subtitle(&format!(
            "{} · {} · {}{}",
            m.drive.image_name().unwrap_or_else(|| "no image".into()),
            if m.drive.connected {
                "connected to host"
            } else {
                "disconnected"
            },
            if m.drive.cdrom { "CD-ROM" } else { "Flash" },
            if m.drive.rw { " (rw)" } else { "" }
        ));
        let (size, free) = if m.storage.size > 0 {
            (m.storage.size, m.storage.free)
        } else {
            m.storage
                .parts
                .values()
                .fold((0, 0), |acc, p| (acc.0 + p.size, acc.1 + p.free))
        };
        self.storage_row.set_subtitle(&format!(
            "{} free of {} · {} image(s)",
            human_bytes(free),
            human_bytes(size),
            m.storage.images.len()
        ));
        match (&m.storage.downloading, &m.storage.uploading) {
            (Some(t), _) | (_, Some(t)) if t.size > 0 => {
                self.transfer_row.set_visible(true);
                self.transfer_row.set_subtitle(&format!(
                    "{}: {} / {} ({:.0}%)",
                    t.name,
                    human_bytes(t.written),
                    human_bytes(t.size),
                    t.written as f64 * 100.0 / t.size as f64
                ));
            }
            _ => self.transfer_row.set_visible(false),
        }
        let names: Vec<String> = m
            .storage
            .images
            .iter()
            .map(|(name, img)| format!("{name} ({})", human_bytes(img.size)))
            .collect();
        let keys: Vec<String> = m.storage.images.keys().cloned().collect();
        if *self.images.borrow() != keys {
            let refs: Vec<&str> = names.iter().map(String::as_str).collect();
            self.image_combo.set_model(Some(&gtk::StringList::new(&refs)));
            if let Some(idx) = m.drive.image_name().and_then(|n| keys.iter().position(|k| *k == n)) {
                self.image_combo.set_selected(idx as u32);
            }
            *self.images.borrow_mut() = keys;
        }
        self.media_combo.set_selected(if m.drive.cdrom { 0 } else { 1 });
        self.rw_switch.set_active(m.drive.rw);
        self.rw_switch.set_sensitive(!m.drive.cdrom);
    }

    fn set_advanced(&self, advanced: bool) {
        for w in &self.advanced {
            w.set_visible(advanced);
        }
        if !advanced {
            self.upload_progress.set_visible(false);
        }
    }
}
