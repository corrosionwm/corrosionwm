use std::time::Duration;

use smithay::{
    backend::{
        renderer::{
            damage::OutputDamageTracker, element::surface::WaylandSurfaceRenderElement,
            gles::GlesRenderer,
        },
        winit::{self, WinitError, WinitEvent, WinitEventLoop, WinitGraphicsBackend},
    },
    output::{Mode, Output, PhysicalProperties, Subpixel},
    reexports::{
        calloop::{
            timer::{TimeoutAction, Timer},
            EventLoop, LoopSignal,
        },
        wayland_server::{protocol::wl_surface::WlSurface, Display},
    },
    utils::{Rectangle, Transform},
};

use crate::{state::Backend, CalloopData, Corrosion};

pub struct WinitData {
    loop_signal: LoopSignal,
}

impl Backend for WinitData {
    fn loop_signal(&self) -> &LoopSignal {
        &self.loop_signal
    }

    fn seat_name(&self) -> String {
        String::from("wayland-0")
    }

    fn early_import(&mut self, _output: &WlSurface) {}

    fn reset_buffers(&self, _surface: &Output) {}
}

pub fn init_winit<BackendData: Backend + 'static>() -> Result<(), Box<dyn std::error::Error>> {
    let mut event_loop = EventLoop::try_new().expect("Unable to create callback loop");
    let mut display = Display::new().expect("Unable to create display :(");
    let backend_data = WinitData {
        loop_signal: event_loop.get_signal(),
    };

    let mut state: Corrosion<WinitData> =
        Corrosion::new(event_loop.handle(), &mut display, backend_data);

    let (mut backend, mut winit) = winit::init()?;

    // This code creates a variable named mode that contains the size and refresh rate of the window.
    let mode = Mode {
        size: backend.window_size().physical_size,
        refresh: 60_000,
    };

    let output = Output::new(
        String::from("Corrosionwm"),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "Corrosionwm".into(), // name of the window manager if you are running a window manager inside a window manager this might matter to you
            model: "Winit".into(),
        },
    );

    let _global = output.create_global::<Corrosion<BackendData>>(&display.handle());
    output.change_current_state(
        Some(mode),
        Some(Transform::Flipped180),
        None,
        Some((0, 0).into()),
    );
    output.set_preferred(mode);

    state.space.map_output(&output, (0, 0));

    let mut damage_tracked_renderer = OutputDamageTracker::from_output(&output);

    // Set the environment variable WAYLAND_DISPLAY to the socket name of the display.
    std::env::set_var("WAYLAND_DISPLAY", &state.socket_name);

    let mut full_redraw = 0u8;

    // This code creates a timer that will be used to redraw the window.
    let timer = Timer::immediate();
    let mut data = CalloopData { state, display };
    event_loop
        .handle()
        .insert_source(timer, move |_, _, data| {
            winit_dispatch(
                &mut backend,
                &mut winit,
                data,
                &output,
                &mut damage_tracked_renderer,
                &mut full_redraw,
            )
            .unwrap();
            TimeoutAction::ToDuration(Duration::from_millis(16))
        })?;

    // aaand we run our loop :3
    event_loop
        .run(None, &mut data, move |_| {})
        .expect("Unable to initialize winit backend");
    Ok(())
}

pub fn winit_dispatch<BackendData: Backend>(
    backend: &mut WinitGraphicsBackend<GlesRenderer>,
    winit: &mut WinitEventLoop,
    data: &mut CalloopData<BackendData>,
    output: &Output,
    damage_tracked_renderer: &mut OutputDamageTracker,
    full_redraw: &mut u8,
) -> Result<(), Box<dyn std::error::Error>> {
    // This code dispatches new events, and if the window is closed, it stops the loop.
    let display = &mut data.display;
    let state = &mut data.state;

    // The callback function passed to dispatch_new_events() is called for every new event
    // that occurred since the last call to dispatch_new_events(). The code above
    // handles two types of events: window resize events and input events. When a new
    // window resize event is received, the output's current state is updated to reflect
    // the new window size. When a new input event is received, it is passed to the
    // state's process_input_event() function.
    let res = winit.dispatch_new_events(|event| match event {
        WinitEvent::Resized { size, .. } => {
            output.change_current_state(
                Some(Mode {
                    size,
                    refresh: 60_000,
                }),
                None,
                None,
                None,
            );
            tracing::debug!("Resized to {:?}", size);
        }
        WinitEvent::Input(event) => state.process_input_event(event),
        _ => (),
    });

    // windowbuilder to set the windows title to "corrosionWM"
    backend.window().set_title("corrosionWM");

    // If the window is closed, stop the loop
    if let Err(WinitError::WindowClosed) = res {
        // Stop the loop

        return Ok(());
    } else {
        res?;
    }

    *full_redraw = full_redraw.saturating_sub(1);

    let size = backend.window_size().physical_size;
    let damage = Rectangle::from_loc_and_size((0, 0), size);

    // This code renders the output, submits the frame, and refreshes the space.
    backend.bind()?;
    smithay::desktop::space::render_output::<_, WaylandSurfaceRenderElement<GlesRenderer>, _, _>(
        output,
        backend.renderer(),
        0,
        [&state.space],
        &[],
        damage_tracked_renderer,
        [0.1, 0.1, 0.1, 1.0],
    )?;
    backend.submit(Some(&[damage]))?;

    // This code sends the frame to the clients.
    state.space.elements().for_each(|window| {
        window.send_frame(
            output,
            state.start_time.elapsed(),
            Some(Duration::ZERO),
            |_, _| Some(output.clone()),
        )
    });

    state.space.refresh();
    display.flush_clients()?;

    Ok(()) // Return Ok if everything went well
}
