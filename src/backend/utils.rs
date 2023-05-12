// Imports
use smithay::{
    backend::renderer::{ImportAll, ImportMem},
    desktop::space::SpaceRenderElements,
    render_elements,
};

use crate::drawing::PointerRenderElement;

// This macro defines a bunch of types and methods that can be used
// to draw a pointer.
render_elements! {
    pub CustomRenderElements<R, E> where
        R: ImportAll + ImportMem;
    Pointer=PointerRenderElement<R>,
    Space=SpaceRenderElements<R, E>
}