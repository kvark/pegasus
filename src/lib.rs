extern crate env_logger;
#[macro_use] extern crate gfx;
extern crate glutin;
extern crate specs;

use std::{thread, time};
use std::sync::mpsc;

pub type Delta = f32;
pub type ColorFormat = gfx::format::Srgba8;
pub type DepthFormat = gfx::format::DepthStencil;

pub const DRAW_PRIORITY: specs::Priority = 10;
pub const DRAW_NAME: &'static str = "draw";


pub trait Shell: 'static + Send {
    fn init_components(&self, &mut specs::World);
    fn init_systems(&mut self, &mut specs::Planner<Delta>);
    fn proceed(&mut self, &specs::World) -> bool;
}

struct App<S> {
    shell: S,
    planner: specs::Planner<Delta>,
    last_time: time::Instant,
}

impl<S: Shell> App<S> {
    fn tick(&mut self) -> bool {
        let elapsed = self.last_time.elapsed();
        self.last_time = time::Instant::now();
        let delta = elapsed.subsec_nanos() as f32 / 1e9 + elapsed.as_secs() as f32;
        self.planner.dispatch(delta);
        self.shell.proceed(self.planner.mut_world())
    }
}

struct EncoderChannel<R: gfx::Resources, C: gfx::CommandBuffer<R>> {
    receiver: mpsc::Receiver<gfx::Encoder<R, C>>,
    sender: mpsc::Sender<gfx::Encoder<R, C>>,
}

pub trait EventHandler {
    fn handle(&mut self, event: glutin::Event) -> bool {
        // quit when Esc is pressed
        match event {
            glutin::Event::KeyboardInput(_, _, Some(glutin::VirtualKeyCode::Escape)) |
            glutin::Event::Closed => false,
            _ => true,
        }
    }
}

// dummy implementation for your convenience
impl EventHandler for () {}

pub struct DrawTargets<R: gfx::Resources> {
    pub color: gfx::handle::RenderTargetView<R, ColorFormat>,
    pub depth: gfx::handle::DepthStencilView<R, DepthFormat>,
}

pub trait Painter<R: gfx::Resources>: 'static + Send {
    type Visual: specs::Component;
    fn draw<C: gfx::CommandBuffer<R>>(&mut self, &Self::Visual, &mut gfx::Encoder<R, C>);
}

struct DrawSystem<R: gfx::Resources, C: gfx::CommandBuffer<R>, P> {
    painter: P,
    channel: EncoderChannel<R, C>,
    targets: DrawTargets<R>,
}

impl<R: 'static + gfx::Resources, C: 'static + Send + gfx::CommandBuffer<R>, P: Painter<R>>
specs::System<Delta> for DrawSystem<R, C, P> {
    fn run(&mut self, arg: specs::RunArg, _: Delta) {
        use specs::Join;
        // get a new command buffer
        let mut encoder = match self.channel.receiver.recv() {
            Ok(r) => r,
            Err(_) => return,
        };
        // fetch visuals
        let vis = arg.fetch(|w| w.read::<P::Visual>());
        // clear screen
        encoder.clear(&self.targets.color, [0.0, 0.0, 0.0, 1.0]);
        encoder.clear_depth(&self.targets.depth, 1.0);
        encoder.clear_stencil(&self.targets.depth, 0);
        // render entities
        for v in (&vis).iter() {
            self.painter.draw(v, &mut encoder);
        }
        // done
        let _ = self.channel.sender.send(encoder);
    }
}

pub fn fly<D: gfx::Device, F: FnMut() -> D::CommandBuffer, S: Shell, P: Painter<D::Resources>, E: EventHandler>(
           window: glutin::Window, mut device: D, mut com_factory: F, targets: DrawTargets<D::Resources>,
           mut shell: S, painter: P, mut event_handler: E)
where D::CommandBuffer: 'static + Send {
    env_logger::init().unwrap();

    let (app_send, dev_recv) = mpsc::channel();
    let (dev_send, app_recv) = mpsc::channel();

    // double-buffering renderers
    for _ in 0..2 {
        let enc = gfx::Encoder::from(com_factory());
        app_send.send(enc).unwrap();
    }

    let enc_chan = EncoderChannel {
        receiver: app_recv,
        sender: app_send,
    };
    let mut app = {
        let draw_sys = DrawSystem {
            painter: painter,
            channel: enc_chan,
            targets: targets,
        };
        let mut w = specs::World::new();
        w.register::<P::Visual>();
        shell.init_components(&mut w);
        let mut plan = specs::Planner::new(w, 4);
        plan.add_system(draw_sys, DRAW_NAME, DRAW_PRIORITY);
        shell.init_systems(&mut plan);
        App {
            shell: shell,
            planner: plan,
            last_time: time::Instant::now(),
        }
    };

    thread::spawn(move || {
        while app.tick() {}
    });

    while let Ok(mut encoder) = dev_recv.recv() {
        // handle events
        for event in window.poll_events() {
            if !event_handler.handle(event) {
                break
            }
        }
        // draw a frame
        encoder.flush(&mut device);
        dev_send.send(encoder).unwrap();
        window.swap_buffers().unwrap();
        device.cleanup();
    }
}
