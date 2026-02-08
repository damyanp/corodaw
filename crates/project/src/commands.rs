use bevy_ecs::prelude::*;

pub trait Command: std::fmt::Debug + Send + Sync {
    fn execute(&self, world: &mut World) -> Option<Box<dyn Command>>;
}

#[derive(Component, Default)]
pub struct CommandManager {
    undo: Vec<Box<dyn Command>>,
    redo: Vec<Box<dyn Command>>,
}

impl CommandManager {
    pub fn execute(&mut self, world: &mut World, command: impl Command) {
        if let Some(undo) = command.execute(world) {
            self.undo.push(undo);
            self.redo.clear();
        }
    }

    pub fn undo(&mut self, world: &mut World) {
        if let Some(command) = self.undo.pop()
            && let Some(redo) = command.execute(world)
        {
            self.redo.push(redo);
        }
    }

    pub fn redo(&mut self, world: &mut World) {
        if let Some(command) = self.redo.pop()
            && let Some(undo) = command.execute(world)
        {
            self.undo.push(undo);
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}
