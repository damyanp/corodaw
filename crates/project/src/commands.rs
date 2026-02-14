use bevy_app::Plugin;
use bevy_ecs::prelude::*;

pub trait Command: std::fmt::Debug + Send + Sync {
    fn execute(&self, world: &mut World) -> Option<Box<dyn Command>>;
}

#[derive(Default)]
pub struct CommandManager {
    undo: Vec<Box<dyn Command>>,
    redo: Vec<Box<dyn Command>>,
}

impl CommandManager {
    pub fn add_undo(&mut self, command: Box<dyn Command>) {
        self.undo.push(command);
        self.redo.clear();
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

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
}

#[derive(Event, Clone, Copy)]
pub enum UndoRedoEvent {
    Undo,
    Redo,
}

pub(crate) struct CommandManagerBevyPlugin;
impl Plugin for CommandManagerBevyPlugin {
    fn build(&self, app: &mut bevy_app::App) {
        app.add_observer(on_undo_redo_event);
    }
}

fn on_undo_redo_event(command: On<UndoRedoEvent>, mut commands: Commands) {
    let command = *command;
    commands.queue(move |world: &mut World| {
        let mut command_manager: CommandManager = world.remove_non_send_resource().unwrap();
        match command {
            UndoRedoEvent::Undo => command_manager.undo(world),
            UndoRedoEvent::Redo => command_manager.redo(world),
        }
        world.insert_non_send_resource(command_manager);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct NoOpCommand;

    impl Command for NoOpCommand {
        fn execute(&self, _world: &mut World) -> Option<Box<dyn Command>> {
            Some(Box::new(NoOpCommand))
        }
    }

    #[test]
    fn clear_empties_stacks() {
        let mut manager = CommandManager::default();
        let mut world = World::new();

        manager.add_undo(Box::new(NoOpCommand));
        manager.undo(&mut world);
        assert!(manager.can_redo());

        manager.add_undo(Box::new(NoOpCommand));
        assert!(manager.can_undo());

        manager.clear();
        assert!(!manager.can_undo());
        assert!(!manager.can_redo());
    }
}
