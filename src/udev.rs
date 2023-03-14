use smithay::{
    backend::{
        drm::{DrmNode, NodeType},
        session::{libseat::LibSeatSession, Session},
        udev::{self, UdevBackend},
    },
    reexports::{
        calloop::{EventLoop, LoopSignal},
        wayland_server::{Display, DisplayHandle},
    },
};

use crate::{state::Backend, CalloopData, Corrosion};

// UdevData is a struct that contains all the information about the system
pub struct UdevData {
    pub session: LibSeatSession,   // The session
    display_handle: DisplayHandle, // The display handle
    primary_gpu: DrmNode,          // The primary gpu
    pub loop_signal: LoopSignal,
}

impl Backend for UdevData {
    fn seat_name(&self) -> String {
        self.session.seat()
    }

    fn loop_signal(&self) -> &LoopSignal {
        &self.loop_signal
    }

    fn early_import(&self, output: &smithay::output::Output) {
        todo!()
    }

    fn reset_buffers(
        &self,
        surface: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
    ) {
        todo!()
    }
}

pub fn initialize_backend() {
    let mut event_loop = EventLoop::try_new().unwrap();
    let mut display = Display::new().unwrap();

    let (mut session, mut notifier) = match LibSeatSession::new() {
        Ok(ret) => ret,
        Err(err) => {
            tracing::error!("{}", err);
            return;
        }
    };

    let primary_gpu = udev::primary_gpu(&session.seat())
        .unwrap()
        .and_then(|f| {
            DrmNode::from_path(f)
                .ok()
                .expect("Not a valid gpu")
                .node_with_type(NodeType::Render)
                .expect("Unable to create drm node")
                .ok()
        })
        .unwrap_or_else(|| {
            udev::all_gpus(&session.seat())
                .unwrap()
                .into_iter()
                .find_map(|f| DrmNode::from_path(f).ok())
                .expect("how the hell did you get a non-gpu device here?")
        });
    tracing::info!("Primary gpu is: {}", primary_gpu);

    let data = UdevData {
        session,
        display_handle: display.handle(),
        primary_gpu,
        loop_signal: event_loop.get_signal(),
    };
    let state = Corrosion::new(event_loop.handle(), &mut display, data);
}

//----------BELOW IS THE SACRED CONVERSATION VIA COMMENTS!!! DO NOT REMOVE-------------
/* hai astrid nyaaaaaaa~ :3
// haiiiiiiiiii electrion :3
// uwu
// nyaaaaaaaaa~
// :3 uwu haiiii ^-^
// ok we should keep this comment
// i'm gonna go to sleep now
// i'll be back in a few hours
// ok
// bye
// bye
// bye
// bye
// bye
// wtfffff
// github copilot is so bad
// that's why i don't use it :kekw:
// jk its actually pretty good
// but it's not perfect
// i'm gonna go to sleep now
// bye
// bye
// bye
// joe
// joe
// joe
// it does not work
// right
// bye
// baiiiiiiiii :3
// github copilot had a stroke
// it keeps removing the sacred comment conversation >:(
// it's because the sacred comment conversation is too sacred, so it's getting removed
// we need more sacredness
// ok
// i'll make it more sacred now
// ok
// bye
// bye
// bye
// bye
*/
