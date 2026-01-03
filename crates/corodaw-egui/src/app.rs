use std::{cell::RefCell, rc::Rc};

use eframe::{EframeWinitApplication, UserEvent};
use smol::LocalExecutor;
use winit::{
    application::ApplicationHandler,
    event::{DeviceEvent, DeviceId, StartCause, WindowEvent},
    event_loop::ActiveEventLoop,
    window::WindowId,
};

use crate::Corodaw;

pub struct App<'a> {
    executor: Rc<LocalExecutor<'a>>,
    corodaw: Rc<RefCell<Corodaw<'a>>>,
    eframe: EframeWinitApplication<'a>,
}

impl<'a> App<'a> {
    pub fn new(
        executor: Rc<LocalExecutor<'a>>,
        corodaw: Rc<RefCell<Corodaw<'a>>>,
        eframe: EframeWinitApplication<'a>,
    ) -> Self {
        Self {
            executor,
            corodaw,
            eframe,
        }
    }
}

impl ApplicationHandler<UserEvent> for App<'_> {
    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        for f in self
            .corodaw
            .borrow()
            .pending_with_active_event_loop_fns
            .replace(Vec::default())
        {
            f(event_loop);
        }

        while self.executor.try_tick() {}

        self.eframe.new_events(event_loop, cause);
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.eframe.resumed(event_loop);
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        self.eframe.user_event(event_loop, event);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if self
            .corodaw
            .borrow()
            .manager
            .window_event(window_id, &event)
        {
            return;
        }

        self.eframe.window_event(event_loop, window_id, event);
    }

    fn device_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        device_id: DeviceId,
        event: DeviceEvent,
    ) {
        self.eframe.device_event(event_loop, device_id, event);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.eframe.about_to_wait(event_loop);
    }

    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        self.eframe.suspended(event_loop);
    }

    fn exiting(&mut self, event_loop: &ActiveEventLoop) {
        self.eframe.exiting(event_loop);
    }

    fn memory_warning(&mut self, event_loop: &ActiveEventLoop) {
        self.eframe.memory_warning(event_loop);
    }
}
