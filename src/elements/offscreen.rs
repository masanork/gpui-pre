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
    }
}

/// See [`offscreen`].
pub struct Offscreen {
    id: OffscreenId,
    child: AnyElement,
    on_read_back: Option<Box<dyn FnOnce(crate::OffscreenReadback, &mut Window, &mut App)>>,
}

impl Offscreen {
    /// Asks for the pixels of the pass when it is painted (see [`Window::read_back_offscreen`]): `f` gets the future that will have them.
    ///
    /// Added in ibisOffice's fork of gpui-pre 0.3.6.
    pub fn on_read_back(mut self, f: impl FnOnce(crate::OffscreenReadback, &mut Window, &mut App) + 'static) -> Self {
        self.on_read_back = Some(Box::new(f));
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
        self.child.prepaint(window, cx);
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
        let child = &mut self.child;
        window.paint_offscreen(id, bounds, |window| child.paint(window, cx));
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
