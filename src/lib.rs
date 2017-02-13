extern crate gfx;
extern crate specs;

use std::{thread, time};
use std::sync::mpsc;

pub type Delta = f32;
pub type Planner = specs::Planner<Delta>;

pub const DRAW_PRIORITY: specs::Priority = 10;
pub const DRAW_NAME: &'static str = "draw";


pub trait Init: 'static {
    type Shell: 'static + Send;
    fn start(self, &mut Planner) -> Self::Shell;
    fn proceed(_: &mut Self::Shell, _: &specs::World) -> bool { true }
}

struct App<I: Init> {
    shell: I::Shell,
    planner: Planner,
    last_time: time::Instant,
}

impl<I: Init> App<I> {
    fn tick(&mut self) -> bool {
        let elapsed = self.last_time.elapsed();
        self.last_time = time::Instant::now();
        let delta = elapsed.subsec_nanos() as f32 / 1e9 + elapsed.as_secs() as f32;
        self.planner.dispatch(delta);
        I::proceed(&mut self.shell, self.planner.mut_world())
    }
}

struct ChannelPair<R: gfx::Resources, C: gfx::CommandBuffer<R>> {
    receiver: mpsc::Receiver<gfx::Encoder<R, C>>,
    sender: mpsc::Sender<gfx::Encoder<R, C>>,
}

pub trait Painter<R: gfx::Resources>: 'static + Send {
    fn draw<'a, C>(&mut self, arg: specs::RunArg, &mut gfx::Encoder<R, C>) where
            C: gfx::CommandBuffer<R>;
}

struct DrawSystem<R: gfx::Resources, C: gfx::CommandBuffer<R>, P> {
    painter: P,
    channel: ChannelPair<R, C>,
}

impl<R, C, P> specs::System<Delta> for DrawSystem<R, C, P>
where
    R: 'static + gfx::Resources,
    C: 'static + Send + gfx::CommandBuffer<R>,
    P: Painter<R>,
{
    fn run(&mut self, arg: specs::RunArg, _: Delta) {
        // get a new command buffer
        let mut encoder = match self.channel.receiver.recv() {
            Ok(r) => r,
            Err(_) => return,
        };
        // render entities
        self.painter.draw(arg, &mut encoder);
        // done
        let _ = self.channel.sender.send(encoder);
    }
}

pub struct Pegasus<D: gfx::Device> {
    pub device: D,
    channel: ChannelPair<D::Resources, D::CommandBuffer>,
    _guard: thread::JoinHandle<()>,
}

pub struct Swing<'a, D: 'a + gfx::Device> {
    device: &'a mut D,
}

impl<'a, D: 'a + gfx::Device> Drop for Swing<'a, D> {
    fn drop(&mut self) {
        self.device.cleanup();
    }
}

impl<D: gfx::Device> Pegasus<D> {
    pub fn new<F, I, P>(init: I, device: D, painter: P, mut com_factory: F)
               -> Pegasus<D> where
        I: Init,
        D::CommandBuffer: 'static + Send, //TODO: remove when gfx forces these bounds
        P: Painter<D::Resources>,
        F: FnMut() -> D::CommandBuffer,
    {
        let (app_send, dev_recv) = mpsc::channel();
        let (dev_send, app_recv) = mpsc::channel();

        // double-buffering renderers
        for _ in 0..2 {
            let enc = gfx::Encoder::from(com_factory());
            app_send.send(enc).unwrap();
        }

        let mut app = {
            let draw_sys = DrawSystem {
                painter: painter,
                channel: ChannelPair {
                    receiver: app_recv,
                    sender: app_send,
                },
            };
            let w = specs::World::new();
            let mut plan = specs::Planner::new(w, 4);
            plan.add_system(draw_sys, DRAW_NAME, DRAW_PRIORITY);
            let shell = init.start(&mut plan);
            App::<I> {
                shell: shell,
                planner: plan,
                last_time: time::Instant::now(),
            }
        };

        Pegasus {
            device: device,
            channel: ChannelPair {
                sender: dev_send,
                receiver: dev_recv,
            },
            _guard: thread::spawn(move || {
                while app.tick() {}
            }),
        }
    }

    pub fn swing(&mut self) -> Option<Swing<D>> {
        match self.channel.receiver.recv() {
            Ok(mut encoder) => {
                // draw a frame
                encoder.flush(&mut self.device);
                if self.channel.sender.send(encoder).is_err() {
                    return None
                }
                Some(Swing {
                    device: &mut self.device,
                })
            },
            Err(_) => None,
        }
    }
}
