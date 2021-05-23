use crate::{
    system::{check_system_change_tick, BoxedSystem, IntoSystem, System, SystemId},
    world::{World, WorldCollection},
};
use std::borrow::Cow;

pub trait ExclusiveSystem: Send + Sync + 'static {
    fn name(&self) -> Cow<'static, str>;

    fn id(&self) -> SystemId;

    fn run(&mut self, worlds: &mut WorldCollection);

    fn initialize(&mut self, worlds: &mut WorldCollection);

    fn check_change_tick(&mut self, change_tick: u32);
}

pub struct ExclusiveSystemFn {
    func: Box<dyn FnMut(&mut WorldCollection) + Send + Sync + 'static>,
    name: Cow<'static, str>,
    id: SystemId,
    last_change_tick: u32,
}

impl ExclusiveSystem for ExclusiveSystemFn {
    fn name(&self) -> Cow<'static, str> {
        self.name.clone()
    }

    fn id(&self) -> SystemId {
        self.id
    }

    fn run(&mut self, worlds: &mut WorldCollection) {
        // The previous value is saved in case this exclusive system is run by another exclusive
        // system
        let saved_last_tick = worlds.last_change_tick;
        worlds.last_change_tick = self.last_change_tick;

        (self.func)(worlds);

        let change_tick = worlds.change_tick.get_mut();
        self.last_change_tick = *change_tick;
        *change_tick += 1;

        worlds.last_change_tick = saved_last_tick;
    }

    fn initialize(&mut self, _: &mut WorldCollection) {}

    fn check_change_tick(&mut self, change_tick: u32) {
        check_system_change_tick(&mut self.last_change_tick, change_tick, self.name.as_ref());
    }
}

pub trait IntoExclusiveSystem<Params, SystemType> {
    fn exclusive_system(self) -> SystemType;
}

impl<F> IntoExclusiveSystem<&mut WorldCollection, ExclusiveSystemFn> for F
where
    F: FnMut(&mut World) + Send + Sync + 'static,
{
    fn exclusive_system(self) -> ExclusiveSystemFn {
        ExclusiveSystemFn {
            func: Box::new(self),
            name: core::any::type_name::<F>().into(),
            id: SystemId::new(),
            last_change_tick: 0,
        }
    }
}

pub struct ExclusiveSystemCoerced {
    system: BoxedSystem<(), ()>,
}

impl ExclusiveSystem for ExclusiveSystemCoerced {
    fn name(&self) -> Cow<'static, str> {
        self.system.name()
    }

    fn id(&self) -> SystemId {
        self.system.id()
    }

    fn run(&mut self, worlds: &mut WorldCollection) {
        self.system.run((), worlds);
        self.system.apply_buffers(worlds);
    }

    fn initialize(&mut self, worlds: &mut WorldCollection) {
        self.system.initialize(worlds);
    }

    fn check_change_tick(&mut self, change_tick: u32) {
        self.system.check_change_tick(change_tick);
    }
}

impl<S, Params, SystemType> IntoExclusiveSystem<(Params, SystemType), ExclusiveSystemCoerced> for S
where
    S: IntoSystem<Params, SystemType>,
    SystemType: System<In = (), Out = ()>,
{
    fn exclusive_system(self) -> ExclusiveSystemCoerced {
        ExclusiveSystemCoerced {
            system: Box::new(self.system()),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        entity::Entity,
        query::With,
        schedule::{Stage, SystemStage},
        system::{Commands, IntoExclusiveSystem, IntoSystem, Query, ResMut},
        world::World,
    };
    #[test]
    fn parallel_with_commands_as_exclusive() {
        let mut world = World::new();

        fn removal(
            mut commands: Commands,
            query: Query<Entity, With<f32>>,
            mut counter: ResMut<usize>,
        ) {
            for entity in query.iter() {
                *counter += 1;
                commands.entity(entity).remove::<f32>();
            }
        }

        let mut stage = SystemStage::parallel().with_system(removal.system());
        world.spawn().insert(0.0f32);
        world.insert_resource(0usize);
        stage.run(&mut world);
        stage.run(&mut world);
        assert_eq!(*world.get_resource::<usize>().unwrap(), 1);

        let mut stage = SystemStage::parallel().with_system(removal.exclusive_system());
        world.spawn().insert(0.0f32);
        world.insert_resource(0usize);
        stage.run(&mut world);
        stage.run(&mut world);
        assert_eq!(*world.get_resource::<usize>().unwrap(), 1);
    }
}
