use std::{cell::Cell, rc::Rc};

use gpui::prelude::*;
use gpui::{
    div, point, px, rgb, size, Bounds, Context, IntoElement, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, Render, ScrollHandle, Styled, Window,
};

pub(super) struct NoteScrollbar {
    handle: ScrollHandle,
    generation: Rc<Cell<u64>>,
    track: Rc<Cell<Bounds<Pixels>>>,
    drag: Rc<Cell<Option<f32>>>,
}

#[derive(Clone, Copy)]
struct Thumb {
    top: f32,
    height: f32,
    travel: f32,
    maximum: f32,
}

fn thumb(track: f32, viewport: f32, maximum: f32, offset: f32) -> Option<Thumb> {
    if track <= 0.0 || viewport <= 0.0 || maximum <= 0.0 {
        return None;
    }
    let height = (track * viewport / (viewport + maximum))
        .max(24.0)
        .min(track);
    let travel = track - height;
    Some(Thumb {
        top: (-offset / maximum).clamp(0.0, 1.0) * travel,
        height,
        travel,
        maximum,
    })
}

fn geometry(handle: &ScrollHandle, bounds: Bounds<Pixels>) -> Option<Thumb> {
    thumb(
        bounds.size.height.into(),
        handle.bounds().size.height.into(),
        handle.max_offset().y.into(),
        handle.offset().y.into(),
    )
}

fn scroll_to(handle: &ScrollHandle, thumb: Thumb, top: f32, window: &mut Window) {
    if thumb.travel > 0.0 {
        let mut offset = handle.offset();
        offset.y = px(-top.clamp(0.0, thumb.travel) / thumb.travel * thumb.maximum);
        handle.set_offset(offset);
        window.refresh();
    }
}

impl NoteScrollbar {
    pub(super) fn new(handle: ScrollHandle, generation: Rc<Cell<u64>>) -> Self {
        Self {
            handle,
            generation,
            track: Rc::new(Cell::new(Bounds::default())),
            drag: Rc::new(Cell::new(None)),
        }
    }
}

impl Render for NoteScrollbar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let track = self.track.clone();
        let handle = self.handle.clone();
        let drag = self.drag.clone();
        let generation = self.generation.clone();
        div()
            .id("note-scrollbar")
            .w(px(12.0))
            .h_full()
            .flex_shrink_0()
            .bg(rgb(0x111214))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    this.generation.set(this.generation.get().wrapping_add(1));
                    let bounds = this.track.get();
                    let Some(thumb) = geometry(&this.handle, bounds) else {
                        return;
                    };
                    let y = f32::from(event.position.y - bounds.origin.y);
                    let grab = if y >= thumb.top && y <= thumb.top + thumb.height {
                        y - thumb.top
                    } else {
                        let grab = thumb.height / 2.0;
                        scroll_to(&this.handle, thumb, y - grab, window);
                        grab
                    };
                    this.drag.set(Some(grab));
                    window.refresh();
                }),
            )
            .child(
                gpui::canvas(
                    move |bounds, _, _| {
                        track.set(bounds);
                    },
                    move |bounds, _, window, _| {
                        if let Some(thumb) = geometry(&handle, bounds) {
                            window.paint_quad(
                                gpui::fill(
                                    Bounds::new(
                                        point(
                                            bounds.origin.x + px(3.0),
                                            bounds.origin.y + px(thumb.top),
                                        ),
                                        size(px(6.0), px(thumb.height)),
                                    ),
                                    rgb(if drag.get().is_some() {
                                        0xa1a1aa
                                    } else {
                                        0x52525b
                                    }),
                                )
                                .corner_radii(px(3.0)),
                            );
                        }
                        let move_handle = handle.clone();
                        let move_drag = drag.clone();
                        let move_generation = generation.clone();
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                            if !phase.capture() {
                                return;
                            }
                            let Some(grab) = move_drag.get() else { return };
                            if event.pressed_button != Some(MouseButton::Left) {
                                move_drag.set(None);
                                window.refresh();
                                return;
                            }
                            if let Some(thumb) = geometry(&move_handle, bounds) {
                                move_generation.set(move_generation.get().wrapping_add(1));
                                scroll_to(
                                    &move_handle,
                                    thumb,
                                    f32::from(event.position.y - bounds.origin.y) - grab,
                                    window,
                                );
                                cx.stop_propagation();
                            }
                        });
                        let up_drag = drag.clone();
                        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, _| {
                            if phase.capture()
                                && event.button == MouseButton::Left
                                && up_drag.take().is_some()
                            {
                                window.refresh();
                            }
                        });
                    },
                )
                .size_full(),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::thumb;

    #[test]
    fn reserves_track_without_overflow() {
        assert!(thumb(200.0, 200.0, 0.0, 0.0).is_none());
        assert!(thumb(0.0, 200.0, 200.0, 0.0).is_none());
    }

    #[test]
    fn scales_and_clamps_thumb() {
        let top = thumb(200.0, 200.0, 200.0, 20.0).unwrap();
        assert_eq!(top.height, 100.0);
        assert_eq!(top.top, 0.0);
        assert_eq!(thumb(200.0, 200.0, 200.0, -300.0).unwrap().top, 100.0);
        assert_eq!(thumb(200.0, 200.0, 200.0, -100.0).unwrap().top, 50.0);
    }

    #[test]
    fn minimum_thumb_fits_short_tracks() {
        assert_eq!(thumb(200.0, 200.0, 10000.0, 0.0).unwrap().height, 24.0);
        let short = thumb(12.0, 200.0, 10000.0, -100.0).unwrap();
        assert_eq!(short.height, 12.0);
        assert_eq!(short.travel, 0.0);
    }
}
