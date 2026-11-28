// Added by ibis inc. for ibisOffice (2026-11-08): the `Offscreen` element (see `Window::paint_offscreen`).
// Modified by ibis inc. for ibisOffice (2026-11-23): `Offscreen::on_read_back` (see `Window::read_back_offscreen`).
use crate::{
    AnyElement, App, Bounds, Element, GlobalElementId, InspectorElementId, IntoElement, LayoutId,
    OffscreenId, Pixels, Window,
};

/// An element that lays out like `child`, but paints it into the offscreen texture `id` instead of the window
/// (see [`Window::paint_offscreen`]). Draw the texture with [`Window::paint_textured_quad`], for example from a
/// [`canvas`](crate::canvas) painted after it, with the bounds this element has.
///
/// Added in ibisOffice's fork of gpui-pre 0.3.6.
pub fn offscreen(id: OffscreenId, child: impl IntoElement) -> Offscreen {
    Offscreen {
        id,
        child: child.into_any_element(),
        on_read_back: None,
        cache_key: None,
        reused: false,
    }
}

/// See [`offscreen`].
pub struct Offscreen {
    id: OffscreenId,
    child: AnyElement,
    on_read_back: Option<Box<dyn FnOnce(crate::OffscreenReadback, &mut Window, &mut App)>>,
    cache_key: Option<u64>,
    reused: bool,
}

impl Offscreen {
    /// Asks for the pixels of the pass when it is painted (see [`Window::read_back_offscreen`]): `f` gets the future that will have them.
    ///
    /// Added in ibisOffice's fork of gpui-pre 0.3.6.
    pub fn on_read_back(mut self, f: impl FnOnce(crate::OffscreenReadback, &mut Window, &mut App) + 'static) -> Self {
        self.on_read_back = Some(Box::new(f));
        self
    }

    /// Reuses the previously painted offscreen texture when `key` and the window scale factor match, skipping child layout and paint.
    /// Callers must include the element's size and all visual inputs in `key`, and place this element inside a container with explicit size.
    pub fn cached(mut self, key: u64) -> Self {
        self.cache_key = Some(key);
        self
    }
}
impl Element for Offscreen {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<crate::ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        if let Some(key) = self.cache_key {
            if self.on_read_back.is_none() && window.can_reuse_offscreen(self.id, key) {
                self.reused = true;
                let style = crate::Style {
                    size: crate::size(crate::relative(1.0).into(), crate::relative(1.0).into()),
                    ..Default::default()
                };
                return (window.request_layout(style, None, cx), ());
            }
        }
        self.reused = false;
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) {
        if !self.reused {
            self.child.prepaint(window, cx);
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let id = self.id;
        if self.reused && self.on_read_back.is_none() {
            window.retain_offscreen(id);
            return;
        }
        let child = &mut self.child;
        window.paint_offscreen(id, bounds, |window| child.paint(window, cx));
        if let Some(key) = self.cache_key {
            if bounds.size.width > crate::px(0.0) && bounds.size.height > crate::px(0.0) {
                window.record_cached_offscreen(id, key);
            }
        }
        if let Some(f) = self.on_read_back.take() {
            let readback = window.read_back_offscreen(id);
            f(readback, window, cx);
        }
    }
}

impl IntoElement for Offscreen {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

/// Caches the painted GPU primitives of `child` under `id` and `key`, replaying them translated to `bounds.origin` on subsequent frames.
pub fn cached_paint(id: u64, key: u64, child: impl IntoElement) -> CachedPaint {
    CachedPaint {
        id,
        key,
        opacity: 1.0,
        child: child.into_any_element(),
        reused: false,
    }
}

/// See [`cached_paint`].
pub struct CachedPaint {
    id: u64,
    key: u64,
    opacity: f32,
    child: AnyElement,
    reused: bool,
}

impl CachedPaint {
    /// Scales the alpha of all cached primitives by `opacity` on replay (captures at full opacity on the initial frame).
    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity.clamp(0.0, 1.0);
        self
    }
}

impl Element for CachedPaint {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<crate::ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        if window.can_reuse_cached_paint(self.id, self.key) {
            self.reused = true;
            let style = crate::Style {
                size: crate::size(crate::relative(1.0).into(), crate::relative(1.0).into()),
                ..Default::default()
            };
            return (window.request_layout(style, None, cx), ());
        }
        self.reused = false;
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) {
        if !self.reused {
            self.child.prepaint(window, cx);
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        if self.reused {
            window.replay_cached_paint(self.id, bounds.origin, self.opacity);
            return;
        }
        if self.opacity < 1.0 {
            window.with_element_opacity(Some(self.opacity), |window| {
                self.child.paint(window, cx);
            });
            return;
        }
        let start_op = window.current_scene_op_len();
        self.child.paint(window, cx);
        window.record_cached_paint(self.id, self.key, bounds.origin, start_op);
    }
}

impl IntoElement for CachedPaint {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}
