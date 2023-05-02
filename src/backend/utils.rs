use smithay::{
    backend::renderer::{
        ImportAll, ImportMem,
    },
    desktop::space::SpaceRenderElements,
    render_elements,
};

use crate::drawing::PointerRenderElement;

render_elements! {
    pub CustomRenderElements<R, E> where
        R: ImportAll + ImportMem;
    Pointer=PointerRenderElement<R>,
    Space=SpaceRenderElements<R, E>
}
