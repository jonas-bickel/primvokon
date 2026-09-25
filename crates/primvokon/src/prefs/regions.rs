//! Region editor: draw watch regions as rectangles on a live snapshot of the host screen.
//!
//! The dialog shows the current frame with a transparent drawing layer on top. A drag creates a
//! rectangle; *Undo* removes the last one, *Clear* all of them, *Save* writes the regions (in
//! stream pixels, origin top-left) back to the caller.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::glib;
use pikvm::models::SnapshotOptions;
use primvokon_core::settings::Region;

use crate::state::state;
use crate::util::{self, spawn_result};

/// Minimum size in image pixels for a drag to count as a region.
const MIN_REGION_PX: f64 = 4.0;

/// Where the image sits inside the drawing area when shown with `ContentFit::Contain`.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Viewport {
    scale: f64,
    offset_x: f64,
    offset_y: f64,
    image_w: f64,
    image_h: f64,
}

impl Viewport {
    fn fit(image_w: f64, image_h: f64, widget_w: f64, widget_h: f64) -> Self {
        let scale = (widget_w / image_w).min(widget_h / image_h).max(f64::EPSILON);
        Self {
            scale,
            offset_x: (widget_w - image_w * scale) / 2.0,
            offset_y: (widget_h - image_h * scale) / 2.0,
            image_w,
            image_h,
        }
    }

    /// Widget coordinates → image pixels, clamped to the frame.
    fn to_image(self, x: f64, y: f64) -> (f64, f64) {
        let ix = ((x - self.offset_x) / self.scale).clamp(0.0, self.image_w);
        let iy = ((y - self.offset_y) / self.scale).clamp(0.0, self.image_h);
        (ix, iy)
    }

    /// Image pixels → widget coordinates.
    fn to_widget(self, ix: f64, iy: f64) -> (f64, f64) {
        (self.offset_x + ix * self.scale, self.offset_y + iy * self.scale)
    }
}

/// Builds a [`Region`] from two image-space corners in any order; `None` when too small.
fn region_from_corners(a: ImagePoint, b: ImagePoint) -> Option<Region> {
    let left = a.0.min(b.0);
    let top = a.1.min(b.1);
    let width = (a.0 - b.0).abs();
    let height = (a.1 - b.1).abs();
    (width >= MIN_REGION_PX && height >= MIN_REGION_PX).then(|| Region {
        left: left.round() as u32,
        top: top.round() as u32,
        width: width.round() as u32,
        height: height.round() as u32,
    })
}

/// A point in image pixels.
type ImagePoint = (f64, f64);

struct Editor {
    regions: RefCell<Vec<Region>>,
    /// Drag in progress: start corner and current corner, in image pixels.
    dragging: RefCell<Option<(ImagePoint, ImagePoint)>>,
    texture: gtk::gdk::Texture,
}

impl Editor {
    fn viewport(&self, widget_w: f64, widget_h: f64) -> Viewport {
        Viewport::fit(
            self.texture.width() as f64,
            self.texture.height() as f64,
            widget_w,
            widget_h,
        )
    }

    fn draw(&self, cr: &gtk::cairo::Context, w: i32, h: i32) {
        let vp = self.viewport(w as f64, h as f64);
        // Dim everything outside the regions so the watched areas stand out.
        let regions = self.regions.borrow();
        if !regions.is_empty() {
            cr.set_source_rgba(0.0, 0.0, 0.0, 0.35);
            let _ = cr.paint();
        }
        for (i, r) in regions.iter().enumerate() {
            let (x, y) = vp.to_widget(r.left as f64, r.top as f64);
            let (x2, y2) = vp.to_widget((r.left + r.width) as f64, (r.top + r.height) as f64);
            cr.set_operator(gtk::cairo::Operator::Clear);
            cr.rectangle(x, y, x2 - x, y2 - y);
            let _ = cr.fill();
            cr.set_operator(gtk::cairo::Operator::Over);
            cr.set_source_rgba(0.2, 0.6, 1.0, 0.9);
            cr.set_line_width(2.0);
            cr.rectangle(x, y, x2 - x, y2 - y);
            let _ = cr.stroke();
            cr.set_font_size(14.0);
            cr.move_to(x + 4.0, y + 16.0);
            let _ = cr.show_text(&format!("{}", i + 1));
        }
        if let Some((a, b)) = *self.dragging.borrow() {
            let (x, y) = vp.to_widget(a.0.min(b.0), a.1.min(b.1));
            let (x2, y2) = vp.to_widget(a.0.max(b.0), a.1.max(b.1));
            cr.set_source_rgba(1.0, 0.8, 0.2, 0.25);
            cr.rectangle(x, y, x2 - x, y2 - y);
            let _ = cr.fill();
            cr.set_source_rgba(1.0, 0.8, 0.2, 0.9);
            cr.set_line_width(1.5);
            cr.rectangle(x, y, x2 - x, y2 - y);
            let _ = cr.stroke();
        }
    }
}

/// Fetches a snapshot and opens the editor. `on_save` receives the regions when the user saves.
pub fn open(parent: &impl IsA<gtk::Widget>, initial: Vec<Region>, on_save: impl Fn(Vec<Region>) + 'static) {
    let Some(client) = state().connection.client() else {
        util::toast("Connect to the PiKVM first to draw regions on a snapshot");
        return;
    };
    let parent = parent.clone();
    let on_save = Rc::new(on_save);
    spawn_result(
        "Snapshot",
        async move {
            let opts = SnapshotOptions {
                allow_offline: true,
                ..Default::default()
            };
            Ok(client.streamer().snapshot(&opts).await?)
        },
        move |jpeg| match gtk::gdk::Texture::from_bytes(&glib::Bytes::from(&jpeg[..])) {
            Ok(texture) => present(&parent, texture, initial, on_save),
            Err(e) => util::toast(&format!("Snapshot could not be decoded: {e}")),
        },
    );
}

fn present(
    parent: &impl IsA<gtk::Widget>,
    texture: gtk::gdk::Texture,
    initial: Vec<Region>,
    on_save: Rc<dyn Fn(Vec<Region>)>,
) {
    let editor = Rc::new(Editor {
        regions: RefCell::new(initial),
        dragging: RefCell::new(None),
        texture: texture.clone(),
    });

    let dialog = adw::Dialog::builder()
        .title("Watch regions")
        .content_width(1000)
        .content_height(680)
        .build();
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());

    let picture = gtk::Picture::for_paintable(&texture);
    picture.set_content_fit(gtk::ContentFit::Contain);
    picture.set_can_shrink(true);
    let area = gtk::DrawingArea::builder().hexpand(true).vexpand(true).build();
    area.set_cursor_from_name(Some("crosshair"));
    {
        let editor = editor.clone();
        area.set_draw_func(move |_, cr, w, h| editor.draw(cr, w, h));
    }
    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&picture));
    overlay.add_overlay(&area);
    overlay.set_measure_overlay(&area, false);

    let summary = gtk::Label::builder()
        .halign(gtk::Align::Start)
        .wrap(true)
        .css_classes(["dim-label"])
        .build();
    let refresh_summary = {
        let editor = editor.clone();
        let summary = summary.clone();
        let (fw, fh) = (texture.width(), texture.height());
        Rc::new(move || {
            let regions = editor.regions.borrow();
            let text = if regions.is_empty() {
                format!("Whole screen ({fw}×{fh}) is watched. Drag to draw a region.")
            } else {
                regions
                    .iter()
                    .enumerate()
                    .map(|(i, r)| format!("{}: {},{} {}×{}", i + 1, r.left, r.top, r.width, r.height))
                    .collect::<Vec<_>>()
                    .join("   ")
            };
            summary.set_text(&text);
        })
    };
    refresh_summary();

    let drag = gtk::GestureDrag::new();
    {
        let editor = editor.clone();
        let area = area.clone();
        drag.connect_drag_begin(move |_, x, y| {
            let vp = editor.viewport(area.width() as f64, area.height() as f64);
            let p = vp.to_image(x, y);
            *editor.dragging.borrow_mut() = Some((p, p));
            area.queue_draw();
        });
    }
    {
        let editor = editor.clone();
        let area = area.clone();
        drag.connect_drag_update(move |g, dx, dy| {
            let Some((sx, sy)) = g.start_point() else { return };
            let vp = editor.viewport(area.width() as f64, area.height() as f64);
            let current = vp.to_image(sx + dx, sy + dy);
            if let Some(d) = editor.dragging.borrow_mut().as_mut() {
                d.1 = current;
            }
            area.queue_draw();
        });
    }
    {
        let editor = editor.clone();
        let area = area.clone();
        let refresh = refresh_summary.clone();
        drag.connect_drag_end(move |_, _, _| {
            if let Some((a, b)) = editor.dragging.borrow_mut().take() {
                if let Some(region) = region_from_corners(a, b) {
                    editor.regions.borrow_mut().push(region);
                }
            }
            refresh();
            area.queue_draw();
        });
    }
    area.add_controller(drag);

    let undo = util::button("Undo last");
    {
        let editor = editor.clone();
        let area = area.clone();
        let refresh = refresh_summary.clone();
        undo.connect_clicked(move |_| {
            editor.regions.borrow_mut().pop();
            refresh();
            area.queue_draw();
        });
    }
    let clear = util::button("Clear");
    {
        let editor = editor.clone();
        let area = area.clone();
        let refresh = refresh_summary.clone();
        clear.connect_clicked(move |_| {
            editor.regions.borrow_mut().clear();
            refresh();
            area.queue_draw();
        });
    }
    let save = util::suggested_button("Save");
    {
        let editor = editor.clone();
        let dialog = dialog.clone();
        save.connect_clicked(move |_| {
            on_save(editor.regions.borrow().clone());
            dialog.close();
        });
    }
    let buttons = util::button_box(&[&undo, &clear, &save]);
    buttons.set_halign(gtk::Align::End);

    let footer = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    footer.set_margin_top(8);
    footer.set_margin_bottom(8);
    footer.set_margin_start(12);
    footer.set_margin_end(12);
    summary.set_hexpand(true);
    footer.append(&summary);
    footer.append(&buttons);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&overlay);
    content.append(&footer);
    toolbar.set_content(Some(&content));
    dialog.set_child(Some(&toolbar));
    dialog.present(Some(parent));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewport_fits_and_centres_the_image() {
        // 1280×720 frame inside a 640×640 widget: scale 0.5, letterboxed vertically.
        let vp = Viewport::fit(1280.0, 720.0, 640.0, 640.0);
        assert!((vp.scale - 0.5).abs() < 1e-9);
        assert!((vp.offset_x - 0.0).abs() < 1e-9);
        assert!((vp.offset_y - 140.0).abs() < 1e-9);
        assert_eq!(vp.to_image(0.0, 140.0), (0.0, 0.0));
        assert_eq!(vp.to_image(640.0, 500.0), (1280.0, 720.0));
        // Outside the image is clamped to the frame.
        assert_eq!(vp.to_image(-50.0, 0.0), (0.0, 0.0));
        assert_eq!(vp.to_widget(640.0, 360.0), (320.0, 320.0));
    }

    #[test]
    fn regions_are_normalised_and_tiny_drags_ignored() {
        let r = region_from_corners((100.0, 80.0), (20.0, 30.0)).unwrap();
        assert_eq!((r.left, r.top, r.width, r.height), (20, 30, 80, 50));
        assert!(region_from_corners((10.0, 10.0), (12.0, 40.0)).is_none());
    }
}
